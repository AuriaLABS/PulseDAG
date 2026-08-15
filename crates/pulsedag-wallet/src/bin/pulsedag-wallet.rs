use std::{
    collections::HashMap,
    env,
    error::Error,
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use pulsedag_wallet::{
    derive_wallet_key_from_seed, encrypt_wallet_seed, wallet_seed_from_mnemonic, SecretString,
    WalletDerivationBranch, WalletKeystoreFile, WalletNetworkContext, WalletSession,
    WalletUnlockPolicy, WalletWatchOnly, WalletWatchOnlyManifest, WalletWatchOnlyScope,
    WalletWatchOnlySessionExt,
};
use serde::Serialize;

const CLI_UNLOCK_TIMEOUT: Duration = Duration::from_secs(60);
const CLI_UNLOCK_MAX_FAILURES: u32 = 3;
const CLI_UNLOCK_LOCKOUT: Duration = Duration::from_secs(1);

type CliResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug)]
enum Command {
    Restore(RestoreArgs),
    Address(AddressArgs),
    WatchExport(WatchExportArgs),
    WatchImport(WatchImportArgs),
    BackupVerify(BackupVerifyArgs),
}

#[derive(Debug)]
struct RestoreArgs {
    keystore: PathBuf,
    network_profile: String,
    chain_id: String,
}

#[derive(Debug)]
struct AddressArgs {
    keystore: PathBuf,
    account: u32,
    branch: WalletDerivationBranch,
    index: u32,
}

#[derive(Debug)]
struct WatchExportArgs {
    keystore: PathBuf,
    account: u32,
    receive_count: u32,
    change_count: u32,
}

#[derive(Debug)]
struct WatchImportArgs {
    manifest: PathBuf,
}

#[derive(Debug)]
struct BackupVerifyArgs {
    keystore: PathBuf,
    manifest: PathBuf,
}

#[derive(Debug, Serialize)]
struct RestoreOutput {
    network_profile: String,
    chain_id: String,
    account: u32,
    anchor_address: String,
    keystore: String,
}

#[derive(Debug, Serialize)]
struct AddressOutput {
    network_profile: String,
    chain_id: String,
    account: u32,
    branch: &'static str,
    index: u32,
    address: String,
    public_key: String,
    derivation_path: String,
}

#[derive(Debug, Serialize)]
struct WatchImportOutput {
    format: String,
    version: u32,
    network_profile: String,
    chain_id: String,
    account: u32,
    entry_count: usize,
    checksum_hex: String,
    signing_capability: bool,
}

#[derive(Debug, Serialize)]
struct BackupVerifyOutput {
    verified: bool,
    network_profile: String,
    chain_id: String,
    account: u32,
    entry_count: usize,
    checksum_hex: String,
}

struct RestoreSecrets {
    password: SecretString,
    mnemonic: SecretString,
    bip39_passphrase: Option<SecretString>,
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn parse_u32(name: &str, value: &str) -> CliResult<u32> {
    value
        .parse::<u32>()
        .map_err(|_| invalid_input(format!("{name} must be an unsigned 32-bit integer")).into())
}

fn required(flags: &HashMap<String, String>, name: &str) -> CliResult<String> {
    flags
        .get(name)
        .cloned()
        .ok_or_else(|| invalid_input(format!("missing required option --{name}")))
        .map_err(Into::into)
}

fn parse_flags(args: impl Iterator<Item = String>) -> CliResult<HashMap<String, String>> {
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

fn reject_unknown(flags: &HashMap<String, String>, allowed: &[&str]) -> CliResult<()> {
    for key in flags.keys() {
        if !allowed.iter().any(|allowed_key| key == allowed_key) {
            return Err(invalid_input(format!("unknown option --{key}")).into());
        }
    }
    Ok(())
}

fn parse_branch(value: &str) -> CliResult<WalletDerivationBranch> {
    match value {
        "receive" => Ok(WalletDerivationBranch::Receive),
        "change" => Ok(WalletDerivationBranch::Change),
        _ => Err(invalid_input("--branch must be receive or change").into()),
    }
}

fn branch_name(branch: WalletDerivationBranch) -> &'static str {
    match branch {
        WalletDerivationBranch::Receive => "receive",
        WalletDerivationBranch::Change => "change",
    }
}

fn parse_command_from(args: impl Iterator<Item = String>) -> CliResult<Command> {
    let mut args = args;
    let command = args.next().ok_or_else(|| {
        invalid_input(
            "expected command: restore, address, watch-export, watch-import, or backup-verify",
        )
    })?;
    let flags = parse_flags(args)?;
    match command.as_str() {
        "restore" => {
            reject_unknown(&flags, &["keystore", "network-profile", "chain-id"])?;
            Ok(Command::Restore(RestoreArgs {
                keystore: PathBuf::from(required(&flags, "keystore")?),
                network_profile: required(&flags, "network-profile")?,
                chain_id: required(&flags, "chain-id")?,
            }))
        }
        "address" => {
            reject_unknown(&flags, &["keystore", "account", "branch", "index"])?;
            Ok(Command::Address(AddressArgs {
                keystore: PathBuf::from(required(&flags, "keystore")?),
                account: parse_u32("--account", &required(&flags, "account")?)?,
                branch: parse_branch(&required(&flags, "branch")?)?,
                index: parse_u32("--index", &required(&flags, "index")?)?,
            }))
        }
        "watch-export" => {
            reject_unknown(
                &flags,
                &["keystore", "account", "receive-count", "change-count"],
            )?;
            Ok(Command::WatchExport(WatchExportArgs {
                keystore: PathBuf::from(required(&flags, "keystore")?),
                account: parse_u32("--account", &required(&flags, "account")?)?,
                receive_count: parse_u32("--receive-count", &required(&flags, "receive-count")?)?,
                change_count: parse_u32("--change-count", &required(&flags, "change-count")?)?,
            }))
        }
        "watch-import" => {
            reject_unknown(&flags, &["manifest"])?;
            Ok(Command::WatchImport(WatchImportArgs {
                manifest: PathBuf::from(required(&flags, "manifest")?),
            }))
        }
        "backup-verify" => {
            reject_unknown(&flags, &["keystore", "manifest"])?;
            Ok(Command::BackupVerify(BackupVerifyArgs {
                keystore: PathBuf::from(required(&flags, "keystore")?),
                manifest: PathBuf::from(required(&flags, "manifest")?),
            }))
        }
        _ => Err(invalid_input(
            "expected command: restore, address, watch-export, watch-import, or backup-verify",
        )
        .into()),
    }
}

fn parse_command() -> CliResult<Command> {
    parse_command_from(env::args().skip(1))
}

fn strip_line_ending(mut value: String) -> String {
    while value.ends_with('\n') || value.ends_with('\r') {
        value.pop();
    }
    value
}

fn read_line(reader: &mut impl BufRead) -> CliResult<Option<String>> {
    let mut input = String::new();
    if reader.read_line(&mut input)? == 0 {
        return Ok(None);
    }
    Ok(Some(strip_line_ending(input)))
}

fn read_password_from(reader: &mut impl BufRead) -> CliResult<SecretString> {
    let password = read_line(reader)?
        .ok_or_else(|| invalid_input("wallet password must be supplied on stdin"))?;
    if password.is_empty() {
        return Err(invalid_input("wallet password must be supplied on stdin").into());
    }
    Ok(SecretString::new(password))
}

fn read_password_from_stdin() -> CliResult<SecretString> {
    let stdin = io::stdin();
    read_password_from(&mut stdin.lock())
}

fn read_restore_secrets_from(reader: &mut impl BufRead) -> CliResult<RestoreSecrets> {
    let password = read_password_from(reader)?;
    let mnemonic = read_line(reader)?
        .ok_or_else(|| invalid_input("mnemonic must be supplied as the second stdin line"))?;
    if mnemonic.is_empty() {
        return Err(invalid_input("mnemonic must be supplied as the second stdin line").into());
    }
    let bip39_passphrase = match read_line(reader)? {
        Some(value) if !value.is_empty() => Some(SecretString::new(value)),
        _ => None,
    };
    Ok(RestoreSecrets {
        password,
        mnemonic: SecretString::new(mnemonic),
        bip39_passphrase,
    })
}

fn read_restore_secrets_from_stdin() -> CliResult<RestoreSecrets> {
    let stdin = io::stdin();
    read_restore_secrets_from(&mut stdin.lock())
}

fn unlock_policy() -> CliResult<WalletUnlockPolicy> {
    Ok(WalletUnlockPolicy::new(
        CLI_UNLOCK_TIMEOUT,
        CLI_UNLOCK_MAX_FAILURES,
        CLI_UNLOCK_LOCKOUT,
    )?)
}

fn ensure_parent_exists(path: &Path) -> CliResult<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn read_manifest(path: &Path) -> CliResult<WalletWatchOnlyManifest> {
    let bytes = fs::read(path)?;
    let manifest = serde_json::from_slice::<WalletWatchOnlyManifest>(&bytes)
        .map_err(|_| invalid_input("watch-only manifest JSON is invalid"))?;
    manifest.validate()?;
    Ok(manifest)
}

fn unlocked_session(
    keystore: &WalletKeystoreFile,
    password: &SecretString,
) -> CliResult<WalletSession> {
    let mut session = WalletSession::new(unlock_policy()?)?;
    session.unlock(keystore, password)?;
    Ok(session)
}

fn run_restore(args: RestoreArgs, secrets: RestoreSecrets) -> CliResult<RestoreOutput> {
    ensure_parent_exists(&args.keystore)?;
    let network = WalletNetworkContext::new(&args.network_profile, &args.chain_id)?;
    let seed = wallet_seed_from_mnemonic(&secrets.mnemonic, secrets.bip39_passphrase.as_ref())?;
    let anchor =
        derive_wallet_key_from_seed(&seed, &network, 0, WalletDerivationBranch::Receive, 0)?;
    let anchor_address = anchor.address().to_string();
    let envelope = encrypt_wallet_seed(
        &args.network_profile,
        &args.chain_id,
        &anchor_address,
        &seed,
        &secrets.password,
    )?;
    drop(anchor);
    drop(seed);

    let keystore = WalletKeystoreFile::try_acquire(&args.keystore)?;
    keystore.create_new(&envelope)?;

    Ok(RestoreOutput {
        network_profile: args.network_profile,
        chain_id: args.chain_id,
        account: 0,
        anchor_address,
        keystore: args.keystore.to_string_lossy().into_owned(),
    })
}

fn run_address(args: AddressArgs, password: &SecretString) -> CliResult<AddressOutput> {
    let keystore = WalletKeystoreFile::try_acquire(&args.keystore)?;
    let mut session = unlocked_session(&keystore, password)?;
    let identity = session
        .status()
        .identity
        .ok_or_else(|| invalid_input("wallet session did not expose authenticated identity"))?;
    let output = session.with_derived_key(args.account, args.branch, args.index, |derived| {
        AddressOutput {
            network_profile: identity.network_profile,
            chain_id: identity.chain_id,
            account: args.account,
            branch: branch_name(args.branch),
            index: args.index,
            address: derived.address().to_string(),
            public_key: derived.public_key_hex().to_string(),
            derivation_path: derived.derivation_path().to_string(),
        }
    })?;
    session.lock();
    Ok(output)
}

fn run_watch_export(
    args: WatchExportArgs,
    password: &SecretString,
) -> CliResult<WalletWatchOnlyManifest> {
    let scope = WalletWatchOnlyScope::new(args.account, args.receive_count, args.change_count)?;
    let keystore = WalletKeystoreFile::try_acquire(&args.keystore)?;
    let mut session = unlocked_session(&keystore, password)?;
    let manifest = session.export_watch_only_manifest(scope)?;
    session.lock();
    Ok(manifest)
}

fn run_watch_import(args: WatchImportArgs) -> CliResult<WatchImportOutput> {
    let manifest = read_manifest(&args.manifest)?;
    let watch_only = WalletWatchOnly::import(manifest)?;
    let manifest = watch_only.manifest();
    Ok(WatchImportOutput {
        format: manifest.format().to_string(),
        version: manifest.version(),
        network_profile: manifest.network_profile().to_string(),
        chain_id: manifest.chain_id().to_string(),
        account: manifest.account(),
        entry_count: manifest.entries().len(),
        checksum_hex: manifest.checksum_hex().to_string(),
        signing_capability: false,
    })
}

fn run_backup_verify(
    args: BackupVerifyArgs,
    password: &SecretString,
) -> CliResult<BackupVerifyOutput> {
    let manifest = read_manifest(&args.manifest)?;
    let keystore = WalletKeystoreFile::try_acquire(&args.keystore)?;
    let mut session = unlocked_session(&keystore, password)?;
    session.verify_watch_only_manifest(&manifest)?;
    session.lock();
    Ok(BackupVerifyOutput {
        verified: true,
        network_profile: manifest.network_profile().to_string(),
        chain_id: manifest.chain_id().to_string(),
        account: manifest.account(),
        entry_count: manifest.entries().len(),
        checksum_hex: manifest.checksum_hex().to_string(),
    })
}

fn write_json<T: Serialize>(value: &T) -> CliResult<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, value)?;
    out.write_all(b"\n")?;
    Ok(())
}

fn run() -> CliResult<()> {
    match parse_command()? {
        Command::Restore(args) => {
            let secrets = read_restore_secrets_from_stdin()?;
            write_json(&run_restore(args, secrets)?)
        }
        Command::Address(args) => {
            let password = read_password_from_stdin()?;
            write_json(&run_address(args, &password)?)
        }
        Command::WatchExport(args) => {
            let password = read_password_from_stdin()?;
            write_json(&run_watch_export(args, &password)?)
        }
        Command::WatchImport(args) => write_json(&run_watch_import(args)?),
        Command::BackupVerify(args) => {
            let password = read_password_from_stdin()?;
            write_json(&run_backup_verify(args, &password)?)
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("pulsedag-wallet: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn args(values: &[&str]) -> impl Iterator<Item = String> {
        values
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn parser_exposes_no_secret_command_line_options() {
        assert!(parse_command_from(args(&[
            "restore",
            "--keystore",
            "wallet.json",
            "--network-profile",
            "public-testnet",
            "--chain-id",
            "pulsedag-public-testnet",
            "--password",
            "secret"
        ]))
        .is_err());
        assert!(parse_command_from(args(&[
            "restore",
            "--keystore",
            "wallet.json",
            "--network-profile",
            "public-testnet",
            "--chain-id",
            "pulsedag-public-testnet",
            "--mnemonic",
            "secret words"
        ]))
        .is_err());
        assert!(parse_command_from(args(&[
            "restore",
            "--keystore",
            "wallet.json",
            "--network-profile",
            "public-testnet",
            "--chain-id",
            "pulsedag-public-testnet",
            "--seed",
            "00"
        ]))
        .is_err());
        assert!(parse_command_from(args(&[
            "address",
            "--keystore",
            "wallet.json",
            "--account",
            "0",
            "--branch",
            "receive",
            "--index",
            "0",
            "--private-key",
            "00"
        ]))
        .is_err());
    }

    #[test]
    fn restore_secret_input_is_line_framed_and_passphrase_is_optional() {
        let mut with_passphrase = Cursor::new(
            "wallet-password\nabandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about\nbip39 passphrase\n",
        );
        let secrets = read_restore_secrets_from(&mut with_passphrase).expect("restore secrets");
        assert!(secrets.bip39_passphrase.is_some());

        let mut without_passphrase = Cursor::new(
            "wallet-password\nabandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about\n\n",
        );
        let secrets = read_restore_secrets_from(&mut without_passphrase).expect("restore secrets");
        assert!(secrets.bip39_passphrase.is_none());
    }

    #[test]
    fn empty_password_or_mnemonic_fails_closed() {
        let mut empty_password = Cursor::new("\nwords\n\n");
        assert!(read_restore_secrets_from(&mut empty_password).is_err());

        let mut empty_mnemonic = Cursor::new("password\n\n\n");
        assert!(read_restore_secrets_from(&mut empty_mnemonic).is_err());
    }

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
}
