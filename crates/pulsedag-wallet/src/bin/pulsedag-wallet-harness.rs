use std::{
    collections::HashMap,
    env,
    error::Error,
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use pulsedag_core::types::{Transaction, Utxo};
use pulsedag_wallet::{
    build_transaction_plan, derive_wallet_key_from_seed, encrypt_wallet_seed,
    generate_wallet_mnemonic, wallet_seed_from_mnemonic, SecretString, WalletDerivationBranch,
    WalletKeystoreFile, WalletNetworkContext, WalletNetworkIdentity, WalletPlanSigner,
    WalletPlanSigningSessionExt, WalletSession, WalletSpendPolicy, WalletTransactionIntent,
    WalletUnlockPolicy,
};
use serde::{Deserialize, Serialize};

const MAX_INIT_RECEIVE_ADDRESSES: u32 = 64;
const HARNESS_MAX_INPUTS: usize = 64;
const HARNESS_UNLOCK_TIMEOUT: Duration = Duration::from_secs(60);
const HARNESS_UNLOCK_MAX_FAILURES: u32 = 3;
const HARNESS_UNLOCK_LOCKOUT: Duration = Duration::from_secs(1);

type HarnessResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug)]
enum Command {
    Init(InitArgs),
    Sign(SignArgs),
}

#[derive(Debug)]
struct InitArgs {
    keystore: PathBuf,
    network_profile: String,
    chain_id: String,
    receive_count: u32,
}

#[derive(Debug)]
struct SignArgs {
    keystore: PathBuf,
    utxos_file: PathBuf,
    network_profile: String,
    chain_id: String,
    to: String,
    amount: u64,
    fee: u64,
    nonce: u64,
    account: u32,
    branch: WalletDerivationBranch,
    index: u32,
}

#[derive(Debug, Serialize)]
struct InitOutput {
    network_profile: String,
    chain_id: String,
    account: u32,
    receive: Vec<PublicAddress>,
}

#[derive(Debug, Serialize)]
struct PublicAddress {
    index: u32,
    address: String,
    public_key: String,
    derivation_path: String,
}

#[derive(Debug, Deserialize)]
struct AddressUtxosResponse {
    ok: bool,
    data: Option<AddressUtxosData>,
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct AddressUtxosData {
    address: String,
    utxos: Vec<Utxo>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    code: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct RelayEnvelope {
    transaction: Transaction,
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn parse_u32(name: &str, value: &str) -> HarnessResult<u32> {
    value
        .parse::<u32>()
        .map_err(|_| invalid_input(format!("{name} must be an unsigned 32-bit integer")).into())
}

fn parse_u64(name: &str, value: &str) -> HarnessResult<u64> {
    value
        .parse::<u64>()
        .map_err(|_| invalid_input(format!("{name} must be an unsigned 64-bit integer")).into())
}

fn required(flags: &HashMap<String, String>, name: &str) -> HarnessResult<String> {
    flags
        .get(name)
        .cloned()
        .ok_or_else(|| invalid_input(format!("missing required option --{name}")))
        .map_err(Into::into)
}

fn parse_flags(args: impl Iterator<Item = String>) -> HarnessResult<HashMap<String, String>> {
    let mut flags = HashMap::new();
    let mut args = args.peekable();
    while let Some(flag) = args.next() {
        if !flag.starts_with("--") || flag.len() <= 2 {
            return Err(invalid_input(format!("unexpected argument: {flag}")).into());
        }
        let key = flag.trim_start_matches("--").to_string();
        let value = args
            .next()
            .ok_or_else(|| invalid_input(format!("missing value for --{key}")))?;
        if value.starts_with("--") {
            return Err(invalid_input(format!("missing value for --{key}")).into());
        }
        if flags.insert(key.clone(), value).is_some() {
            return Err(invalid_input(format!("duplicate option --{key}")).into());
        }
    }
    Ok(flags)
}

fn reject_unknown(flags: &HashMap<String, String>, allowed: &[&str]) -> HarnessResult<()> {
    for key in flags.keys() {
        if !allowed.iter().any(|allowed_key| key == allowed_key) {
            return Err(invalid_input(format!("unknown option --{key}")).into());
        }
    }
    Ok(())
}

fn parse_branch(value: &str) -> HarnessResult<WalletDerivationBranch> {
    match value {
        "receive" => Ok(WalletDerivationBranch::Receive),
        "change" => Ok(WalletDerivationBranch::Change),
        _ => Err(invalid_input("--branch must be receive or change").into()),
    }
}

fn parse_command() -> HarnessResult<Command> {
    let mut args = env::args().skip(1);
    let command = args
        .next()
        .ok_or_else(|| invalid_input("expected command: init or sign"))?;
    let flags = parse_flags(args)?;
    match command.as_str() {
        "init" => {
            reject_unknown(
                &flags,
                &["keystore", "network-profile", "chain-id", "receive-count"],
            )?;
            let receive_count = parse_u32("--receive-count", &required(&flags, "receive-count")?)?;
            if receive_count == 0 || receive_count > MAX_INIT_RECEIVE_ADDRESSES {
                return Err(invalid_input(format!(
                    "--receive-count must be between 1 and {MAX_INIT_RECEIVE_ADDRESSES}"
                ))
                .into());
            }
            Ok(Command::Init(InitArgs {
                keystore: PathBuf::from(required(&flags, "keystore")?),
                network_profile: required(&flags, "network-profile")?,
                chain_id: required(&flags, "chain-id")?,
                receive_count,
            }))
        }
        "sign" => {
            reject_unknown(
                &flags,
                &[
                    "keystore",
                    "utxos-file",
                    "network-profile",
                    "chain-id",
                    "to",
                    "amount",
                    "fee",
                    "nonce",
                    "account",
                    "branch",
                    "index",
                ],
            )?;
            Ok(Command::Sign(SignArgs {
                keystore: PathBuf::from(required(&flags, "keystore")?),
                utxos_file: PathBuf::from(required(&flags, "utxos-file")?),
                network_profile: required(&flags, "network-profile")?,
                chain_id: required(&flags, "chain-id")?,
                to: required(&flags, "to")?,
                amount: parse_u64("--amount", &required(&flags, "amount")?)?,
                fee: parse_u64("--fee", &required(&flags, "fee")?)?,
                nonce: parse_u64("--nonce", &required(&flags, "nonce")?)?,
                account: parse_u32("--account", &required(&flags, "account")?)?,
                branch: parse_branch(&required(&flags, "branch")?)?,
                index: parse_u32("--index", &required(&flags, "index")?)?,
            }))
        }
        _ => Err(invalid_input("expected command: init or sign").into()),
    }
}

fn read_password_from_stdin() -> HarnessResult<SecretString> {
    let stdin = io::stdin();
    let mut input = String::new();
    stdin.lock().read_line(&mut input)?;
    while input.ends_with('\n') || input.ends_with('\r') {
        input.pop();
    }
    if input.is_empty() {
        return Err(invalid_input("wallet password must be supplied on stdin").into());
    }
    Ok(SecretString::new(input))
}

fn unlock_policy() -> HarnessResult<WalletUnlockPolicy> {
    Ok(WalletUnlockPolicy::new(
        HARNESS_UNLOCK_TIMEOUT,
        HARNESS_UNLOCK_MAX_FAILURES,
        HARNESS_UNLOCK_LOCKOUT,
    )?)
}

fn ensure_parent_exists(path: &Path) -> HarnessResult<()> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn run_init(args: InitArgs, password: &SecretString) -> HarnessResult<InitOutput> {
    ensure_parent_exists(&args.keystore)?;
    let network = WalletNetworkContext::new(&args.network_profile, &args.chain_id)?;
    let mnemonic = generate_wallet_mnemonic()?;
    let seed = wallet_seed_from_mnemonic(&mnemonic, None)?;
    let anchor = derive_wallet_key_from_seed(
        &seed,
        &network,
        0,
        WalletDerivationBranch::Receive,
        0,
    )?;
    let envelope = encrypt_wallet_seed(
        &args.network_profile,
        &args.chain_id,
        anchor.address(),
        &seed,
        password,
    )?;
    let keystore = WalletKeystoreFile::try_acquire(&args.keystore)?;
    keystore.create_new(&envelope)?;

    // The mnemonic and raw binary seed stay inside zeroizing secret types. Once
    // the encrypted keystore is published, derive public addresses only through
    // the bounded v2 session boundary used by production wallet code.
    drop(seed);
    drop(mnemonic);

    let mut session = WalletSession::new(unlock_policy()?)?;
    session.unlock(&keystore, password)?;
    let mut receive = Vec::with_capacity(args.receive_count as usize);
    for index in 0..args.receive_count {
        let public = session.with_derived_key(
            0,
            WalletDerivationBranch::Receive,
            index,
            |derived| PublicAddress {
                index,
                address: derived.address().to_string(),
                public_key: derived.public_key_hex().to_string(),
                derivation_path: derived.derivation_path().to_string(),
            },
        )?;
        receive.push(public);
    }
    session.lock();
    Ok(InitOutput {
        network_profile: args.network_profile,
        chain_id: args.chain_id,
        account: 0,
        receive,
    })
}

fn load_address_utxos(path: &Path, expected_address: &str) -> HarnessResult<Vec<Utxo>> {
    let bytes = fs::read(path)?;
    let response: AddressUtxosResponse = serde_json::from_slice(&bytes)
        .map_err(|_| invalid_input("UTXO file is not a valid address UTXO API response"))?;
    if !response.ok {
        let detail = response
            .error
            .map(|error| format!("{}: {}", error.code, error.message))
            .unwrap_or_else(|| "unknown RPC error".to_string());
        return Err(invalid_input(format!("UTXO API response failed: {detail}")).into());
    }
    let data = response
        .data
        .ok_or_else(|| invalid_input("UTXO API response is missing data"))?;
    if data.address != expected_address {
        return Err(invalid_input("UTXO response address does not match selected signer").into());
    }
    if data.utxos.iter().any(|utxo| utxo.address != expected_address) {
        return Err(invalid_input("UTXO response contains an entry for a different address").into());
    }
    Ok(data.utxos)
}

fn run_sign(args: SignArgs, password: &SecretString) -> HarnessResult<RelayEnvelope> {
    let keystore = WalletKeystoreFile::try_acquire(&args.keystore)?;
    let mut session = WalletSession::new(unlock_policy()?)?;
    session.unlock(&keystore, password)?;

    let identity = session
        .status()
        .identity
        .ok_or_else(|| invalid_input("wallet session did not expose authenticated identity"))?;
    let expected_network = WalletNetworkIdentity::new(&args.network_profile, &args.chain_id)?;
    let observed_network = WalletNetworkIdentity::new(identity.network_profile, identity.chain_id)?;
    expected_network.ensure_matches(&observed_network)?;

    let signer_address = session.with_derived_key(
        args.account,
        args.branch,
        args.index,
        |derived| derived.address().to_string(),
    )?;
    let available_utxos = load_address_utxos(&args.utxos_file, &signer_address)?;
    let intent = WalletTransactionIntent::new(&signer_address, args.to, args.amount, args.fee)?;
    let spend_policy = WalletSpendPolicy::new(args.fee, 10_000, HARNESS_MAX_INPUTS)?;
    let plan = build_transaction_plan(
        expected_network,
        spend_policy,
        intent,
        &available_utxos,
        args.nonce,
    )?;
    let signed = session.sign_transaction_plan(
        &plan,
        WalletPlanSigner::DeterministicV2 {
            account: args.account,
            branch: args.branch,
            index: args.index,
        },
    )?;
    session.lock();
    Ok(RelayEnvelope {
        transaction: signed.transaction,
    })
}

fn write_json<T: Serialize>(value: &T) -> HarnessResult<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, value)?;
    out.write_all(b"\n")?;
    Ok(())
}

fn run() -> HarnessResult<()> {
    let command = parse_command()?;
    let password = read_password_from_stdin()?;
    match command {
        Command::Init(args) => write_json(&run_init(args, &password)?),
        Command::Sign(args) => write_json(&run_sign(args, &password)?),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("pulsedag-wallet-harness: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::types::OutPoint;

    #[test]
    fn branch_parser_is_explicit() {
        assert!(matches!(
            parse_branch("receive").unwrap(),
            WalletDerivationBranch::Receive
        ));
        assert!(matches!(
            parse_branch("change").unwrap(),
            WalletDerivationBranch::Change
        ));
        assert!(parse_branch("external").is_err());
    }

    #[test]
    fn address_utxo_parser_rejects_cross_address_entries() {
        let dir = std::env::temp_dir().join(format!(
            "pulsedag-wallet-harness-utxo-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("utxos.json");
        let response = serde_json::json!({
            "ok": true,
            "data": {
                "address": "pulse1sender",
                "count": 1,
                "utxos": [{
                    "outpoint": {"txid": "11".repeat(32), "index": 0},
                    "address": "pulse1other",
                    "amount": 5,
                    "coinbase": false,
                    "height": 1
                }]
            },
            "error": null,
            "meta": {}
        });
        fs::write(&path, serde_json::to_vec(&response).unwrap()).unwrap();
        assert!(load_address_utxos(&path, "pulse1sender").is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn relay_envelope_serializes_only_transaction() {
        let envelope = RelayEnvelope {
            transaction: Transaction {
                txid: "tx".into(),
                version: 1,
                inputs: vec![],
                outputs: vec![],
                fee: 0,
                nonce: 1,
            },
        };
        let value = serde_json::to_value(envelope).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 1);
        assert!(value.get("transaction").is_some());
        assert!(value.get("password").is_none());
        assert!(value.get("private_key").is_none());
        assert!(value.get("seed").is_none());
    }

    #[test]
    fn utxo_shape_deserializes_public_outpoint_data() {
        let utxo = Utxo {
            outpoint: OutPoint {
                txid: "22".repeat(32),
                index: 3,
            },
            address: "pulse1sender".into(),
            amount: 10,
            coinbase: false,
            height: 7,
        };
        let encoded = serde_json::to_value(&utxo).unwrap();
        assert_eq!(encoded["address"], "pulse1sender");
        assert_eq!(encoded["outpoint"]["index"], 3);
    }
}
