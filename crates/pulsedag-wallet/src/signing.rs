use std::{error::Error, fmt};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use pulsedag_core::{
    compute_txid, compute_txid_v2, signing_message, signing_message_v2, types::Transaction,
    TRANSACTION_VERSION_V2,
};
use serde::{Deserialize, Serialize};

use crate::{
    WalletDerivationBranch, WalletNetworkIdentity, WalletPlanError, WalletProtocolBindingV2,
    WalletReviewSummary, WalletSecretKey, WalletSession, WalletSessionError,
    WalletSigningPreparation, WalletTransactionPlan,
};

pub const WALLET_PROTOCOL_AUTHORIZATION_DOMAIN_V1: &str =
    "PulseDAG:wallet-protocol-authorization:v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletProtocolAuthorizationV1 {
    pub domain: String,
    pub protocol_fingerprint: String,
    pub signature: String,
}

pub fn wallet_protocol_authorization_message_v1(
    binding: &WalletProtocolBindingV2,
    transaction: &Transaction,
) -> Result<Vec<u8>, WalletPlanError> {
    if transaction.version != TRANSACTION_VERSION_V2 {
        return Err(WalletPlanError::InvalidPlanField {
            field: "transaction.version",
            reason: "protocol authorization requires transaction version 2",
        });
    }

    let expected_fingerprint =
        crate::protocol_v2::verify_wallet_v2_node_identity(&binding.identity, &binding.identity)
            .map_err(WalletPlanError::Build)?;
    if expected_fingerprint != binding.fingerprint {
        return Err(WalletPlanError::InvalidPlanField {
            field: "protocol_binding_v2.fingerprint",
            reason: "does not match activated protocol identity",
        });
    }

    let fingerprint = hex::decode(&binding.fingerprint).map_err(|_| {
        WalletPlanError::InvalidPlanField {
            field: "protocol_binding_v2.fingerprint",
            reason: "must be canonical hexadecimal",
        }
    })?;
    if fingerprint.len() != 32 {
        return Err(WalletPlanError::InvalidPlanField {
            field: "protocol_binding_v2.fingerprint",
            reason: "must encode exactly 32 bytes",
        });
    }

    let mut unsigned = transaction.clone();
    unsigned.txid.clear();
    for input in &mut unsigned.inputs {
        input.signature.clear();
    }
    let signing_bytes = signing_message_v2(&unsigned, &binding.identity.chain_id)
        .map_err(WalletPlanError::Build)?;

    let domain = WALLET_PROTOCOL_AUTHORIZATION_DOMAIN_V1.as_bytes();
    let domain_len = u32::try_from(domain.len()).map_err(|_| WalletPlanError::InvalidPlanField {
        field: "protocol_authorization_v1.domain",
        reason: "domain length exceeds u32",
    })?;
    let signing_len =
        u32::try_from(signing_bytes.len()).map_err(|_| WalletPlanError::InvalidPlanField {
            field: "protocol_authorization_v1.signing_message",
            reason: "signing message length exceeds u32",
        })?;

    let mut message =
        Vec::with_capacity(4 + domain.len() + fingerprint.len() + 4 + signing_bytes.len());
    message.extend_from_slice(&domain_len.to_le_bytes());
    message.extend_from_slice(domain);
    message.extend_from_slice(&fingerprint);
    message.extend_from_slice(&signing_len.to_le_bytes());
    message.extend_from_slice(&signing_bytes);
    Ok(message)
}

fn sign_wallet_protocol_authorization_v1(
    signing_key: &SigningKey,
    binding: &WalletProtocolBindingV2,
    transaction: &Transaction,
) -> Result<WalletProtocolAuthorizationV1, WalletPlanError> {
    let message = wallet_protocol_authorization_message_v1(binding, transaction)?;
    Ok(WalletProtocolAuthorizationV1 {
        domain: WALLET_PROTOCOL_AUTHORIZATION_DOMAIN_V1.to_string(),
        protocol_fingerprint: binding.fingerprint.clone(),
        signature: hex::encode(signing_key.sign(&message).to_bytes()),
    })
}

pub fn verify_wallet_protocol_authorization_v1(
    binding: &WalletProtocolBindingV2,
    transaction: &Transaction,
    authorization: &WalletProtocolAuthorizationV1,
) -> Result<(), WalletPlanError> {
    if authorization.domain != WALLET_PROTOCOL_AUTHORIZATION_DOMAIN_V1 {
        return Err(WalletPlanError::InvalidPlanField {
            field: "protocol_authorization_v1.domain",
            reason: "unsupported wallet protocol authorization domain",
        });
    }
    if authorization.protocol_fingerprint != binding.fingerprint {
        return Err(WalletPlanError::InvalidPlanField {
            field: "protocol_authorization_v1.protocol_fingerprint",
            reason: "does not match signed protocol binding",
        });
    }
    let first = transaction.inputs.first().ok_or(WalletPlanError::InvalidPlanField {
        field: "transaction.inputs",
        reason: "protocol authorization requires at least one input",
    })?;
    if transaction
        .inputs
        .iter()
        .any(|input| input.public_key != first.public_key)
    {
        return Err(WalletPlanError::InvalidPlanField {
            field: "transaction.inputs",
            reason: "protocol authorization requires one wallet signing key",
        });
    }

    let public_key: [u8; 32] = hex::decode(&first.public_key)
        .map_err(|_| WalletPlanError::InvalidPublicKey {
            reason: "must be hexadecimal",
        })?
        .try_into()
        .map_err(|_| WalletPlanError::InvalidPublicKey {
            reason: "must encode exactly 32 bytes",
        })?;
    let signature: [u8; 64] = hex::decode(&authorization.signature)
        .map_err(|_| WalletPlanError::InvalidPlanField {
            field: "protocol_authorization_v1.signature",
            reason: "must be hexadecimal",
        })?
        .try_into()
        .map_err(|_| WalletPlanError::InvalidPlanField {
            field: "protocol_authorization_v1.signature",
            reason: "must encode exactly 64 bytes",
        })?;

    let verifying_key =
        VerifyingKey::from_bytes(&public_key).map_err(|_| WalletPlanError::InvalidPublicKey {
            reason: "is not a valid Ed25519 public key",
        })?;
    let message = wallet_protocol_authorization_message_v1(binding, transaction)?;
    verifying_key
        .verify(&message, &Signature::from_bytes(&signature))
        .map_err(|_| WalletPlanError::InvalidPlanField {
            field: "protocol_authorization_v1.signature",
            reason: "does not authorize the bound protocol identity",
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletPlanSigner {
    LegacyV1,
    DeterministicV2 {
        account: u32,
        branch: WalletDerivationBranch,
        index: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalletSignedTransaction {
    pub network: WalletNetworkIdentity,
    pub review: WalletReviewSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_binding_v2: Option<WalletProtocolBindingV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_authorization_v1: Option<WalletProtocolAuthorizationV1>,
    pub transaction: Transaction,
}

#[derive(Debug)]
pub enum WalletPlanSigningError {
    Session(WalletSessionError),
    Plan(WalletPlanError),
}

impl fmt::Display for WalletPlanSigningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session(error) => write!(f, "wallet session signing failed: {error}"),
            Self::Plan(error) => write!(f, "wallet transaction-plan signing failed: {error}"),
        }
    }
}

impl Error for WalletPlanSigningError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Session(error) => Some(error),
            Self::Plan(error) => Some(error),
        }
    }
}

impl From<WalletSessionError> for WalletPlanSigningError {
    fn from(value: WalletSessionError) -> Self {
        Self::Session(value)
    }
}

impl From<WalletPlanError> for WalletPlanSigningError {
    fn from(value: WalletPlanError) -> Self {
        Self::Plan(value)
    }
}

pub trait WalletPlanSigningSessionExt {
    fn sign_transaction_plan(
        &self,
        plan: &WalletTransactionPlan,
        signer: WalletPlanSigner,
    ) -> Result<WalletSignedTransaction, WalletPlanSigningError>;
}

impl WalletPlanSigningSessionExt for WalletSession {
    fn sign_transaction_plan(
        &self,
        plan: &WalletTransactionPlan,
        signer: WalletPlanSigner,
    ) -> Result<WalletSignedTransaction, WalletPlanSigningError> {
        sign_transaction_plan(self, plan, signer)
    }
}

pub fn sign_transaction_plan(
    session: &WalletSession,
    plan: &WalletTransactionPlan,
    signer: WalletPlanSigner,
) -> Result<WalletSignedTransaction, WalletPlanSigningError> {
    let network = unlocked_network_identity(session)?;
    match signer {
        WalletPlanSigner::LegacyV1 => session
            .with_unlocked_secret(|secret| sign_with_secret(plan, &network, secret))
            .map_err(WalletPlanSigningError::Session)?
            .map_err(WalletPlanSigningError::Plan),
        WalletPlanSigner::DeterministicV2 {
            account,
            branch,
            index,
        } => session
            .with_derived_key(account, branch, index, |derived| {
                sign_with_secret(plan, &network, derived.secret_key())
            })
            .map_err(WalletPlanSigningError::Session)?
            .map_err(WalletPlanSigningError::Plan),
    }
}

fn unlocked_network_identity(
    session: &WalletSession,
) -> Result<WalletNetworkIdentity, WalletPlanSigningError> {
    let identity = session
        .status()
        .identity
        .ok_or(WalletPlanSigningError::Session(WalletSessionError::Locked))?;
    WalletNetworkIdentity::new(identity.network_profile, identity.chain_id)
        .map_err(WalletPlanSigningError::Plan)
}

fn sign_with_secret(
    plan: &WalletTransactionPlan,
    network: &WalletNetworkIdentity,
    secret: &WalletSecretKey,
) -> Result<WalletSignedTransaction, WalletPlanError> {
    let signing_key = SigningKey::from_bytes(secret.expose_secret());
    let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());
    let prepared = plan.prepare_signing(network, &public_key_hex)?;
    let message =
        hex::decode(&prepared.signing_message).map_err(|_| WalletPlanError::InvalidPlanField {
            field: "signing_message",
            reason: "prepared signing message is not canonical hexadecimal",
        })?;
    let signature_hex = hex::encode(signing_key.sign(&message).to_bytes());
    let protocol_authorization_v1 = match &prepared.protocol_binding_v2 {
        Some(binding) => Some(sign_wallet_protocol_authorization_v1(
            &signing_key,
            binding,
            &prepared.transaction,
        )?),
        None => None,
    };
    finalize_signed_transaction(prepared, signature_hex, protocol_authorization_v1)
}

fn finalize_signed_transaction(
    prepared: WalletSigningPreparation,
    signature_hex: String,
    protocol_authorization_v1: Option<WalletProtocolAuthorizationV1>,
) -> Result<WalletSignedTransaction, WalletPlanError> {
    let WalletSigningPreparation {
        network,
        review,
        protocol_binding_v2,
        mut transaction,
        signing_message: expected_signing_message,
        ..
    } = prepared;
    let actual_signing_message = match &protocol_binding_v2 {
        Some(binding) => hex::encode(
            signing_message_v2(&transaction, &binding.identity.chain_id)
                .map_err(WalletPlanError::Build)?,
        ),
        None => hex::encode(signing_message(&transaction)),
    };
    if actual_signing_message != expected_signing_message {
        return Err(WalletPlanError::InvalidPlanField {
            field: "signing_message",
            reason: "changed after reviewed signing preparation",
        });
    }
    if let (Some(binding), Some(authorization)) =
        (&protocol_binding_v2, &protocol_authorization_v1)
    {
        verify_wallet_protocol_authorization_v1(binding, &transaction, authorization)?;
    } else if protocol_binding_v2.is_some() || protocol_authorization_v1.is_some() {
        return Err(WalletPlanError::InvalidPlanField {
            field: "protocol_authorization_v1",
            reason: "must be present exactly when protocol_binding_v2 is present",
        });
    }

    for input in &mut transaction.inputs {
        input.signature = signature_hex.clone();
    }
    transaction.txid = match &protocol_binding_v2 {
        Some(binding) => compute_txid_v2(&transaction, &binding.identity.chain_id)
            .map_err(WalletPlanError::Build)?,
        None => compute_txid(&transaction),
    };
    Ok(WalletSignedTransaction {
        network,
        review,
        protocol_binding_v2,
        protocol_authorization_v1,
        transaction,
    })
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, thread, time::Duration};

    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    use pulsedag_core::{
        address_from_public_key, compute_txid_v2, signing_message_v2,
        types::{OutPoint, Utxo},
        ProtocolActivationIdentity,
    };
    use rand::{rngs::OsRng, RngCore};

    use super::*;
    use crate::{
        build_transaction_plan,
        deterministic::{derive_wallet_key_from_seed, WalletNetworkContext},
        keystore_crypto::{encrypt_private_key_with_kdf_costs, KeystoreKdfCosts},
        keystore_seed::{encrypt_wallet_seed_with_kdf_costs, SeedKeystoreKdfCosts},
        wallet_seed_from_mnemonic, SecretString, WalletKeystoreFile, WalletSpendPolicy,
        WalletTransactionIntent, WalletUnlockPolicy, ED25519_SECRET_KEY_BYTES,
        KEYSTORE_KDF_MIN_ITERATIONS, KEYSTORE_KDF_MIN_LANES, KEYSTORE_KDF_MIN_MEMORY_KIB,
    };

    const MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const PASSWORD: &str = "local-plan-signing-password";
    const NETWORK_PROFILE: &str = "public-testnet-v2.4.0-candidate";
    const CHAIN_ID: &str = "pulsedag-public-testnet-v2.4.0-candidate";

    fn test_dir(label: &str) -> PathBuf {
        let mut random = [0_u8; 8];
        OsRng.fill_bytes(&mut random);
        let dir = std::env::temp_dir().join(format!(
            "pulsedag-local-signing-{label}-{}-{}",
            std::process::id(),
            hex::encode(random)
        ));
        fs::create_dir(&dir).expect("create local signing test directory");
        dir
    }

    fn policy(timeout: Duration) -> WalletUnlockPolicy {
        WalletUnlockPolicy::new(timeout, 3, Duration::from_secs(1)).expect("policy")
    }

    fn v1_fixture(label: &str) -> (PathBuf, WalletKeystoreFile, String) {
        let dir = test_dir(label);
        let path = dir.join("wallet.json");
        let bytes = [0x55; ED25519_SECRET_KEY_BYTES];
        let secret = WalletSecretKey::from_bytes(bytes);
        let signing_key = SigningKey::from_bytes(&bytes);
        let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());
        let address = address_from_public_key(&public_key_hex);
        let envelope = encrypt_private_key_with_kdf_costs(
            NETWORK_PROFILE,
            CHAIN_ID,
            &address,
            &secret,
            &SecretString::new(PASSWORD),
            KeystoreKdfCosts::new(
                KEYSTORE_KDF_MIN_MEMORY_KIB,
                KEYSTORE_KDF_MIN_ITERATIONS,
                KEYSTORE_KDF_MIN_LANES,
            ),
        )
        .expect("encrypt v1 fixture");
        let file = WalletKeystoreFile::try_acquire(&path).expect("acquire v1 fixture");
        file.create_new(&envelope).expect("persist v1 fixture");
        (dir, file, address)
    }

    fn seed_fixture(label: &str) -> (PathBuf, WalletKeystoreFile, String) {
        let dir = test_dir(label);
        let path = dir.join("wallet.json");
        let seed = wallet_seed_from_mnemonic(&SecretString::new(MNEMONIC), None).expect("seed");
        let network = WalletNetworkContext::new(NETWORK_PROFILE, CHAIN_ID).expect("network");
        let anchor =
            derive_wallet_key_from_seed(&seed, &network, 0, WalletDerivationBranch::Receive, 0)
                .expect("anchor")
                .address()
                .to_string();
        let envelope = encrypt_wallet_seed_with_kdf_costs(
            NETWORK_PROFILE,
            CHAIN_ID,
            &anchor,
            &seed,
            &SecretString::new(PASSWORD),
            SeedKeystoreKdfCosts::new(
                KEYSTORE_KDF_MIN_MEMORY_KIB,
                KEYSTORE_KDF_MIN_ITERATIONS,
                KEYSTORE_KDF_MIN_LANES,
            ),
        )
        .expect("encrypt seed fixture");
        let file = WalletKeystoreFile::try_acquire(&path).expect("acquire seed fixture");
        file.create_new(&envelope).expect("persist seed fixture");
        (dir, file, anchor)
    }

    fn recipient_address() -> String {
        address_from_public_key(&"cd".repeat(32))
    }

    fn transaction_plan(from: &str, chain_id: &str) -> WalletTransactionPlan {
        let network = WalletNetworkIdentity::new(NETWORK_PROFILE, chain_id).expect("network");
        let spend_policy = WalletSpendPolicy::new(100, 1_000, 8).expect("spend policy");
        let intent = WalletTransactionIntent::new(from, recipient_address(), 400, 10)
            .expect("transaction intent");
        let available = vec![Utxo {
            outpoint: OutPoint {
                txid: "11".repeat(32),
                index: 0,
            },
            address: from.to_string(),
            amount: 1_000,
            coinbase: false,
            height: 10,
        }];
        build_transaction_plan(network, spend_policy, intent, &available, 42).expect("plan")
    }

    fn verify_signed_transaction(transaction: &Transaction) {
        assert_eq!(transaction.txid, compute_txid(transaction));
        let first = transaction.inputs.first().expect("signed input");
        let public_key_bytes: [u8; 32] = hex::decode(&first.public_key)
            .expect("public key hex")
            .try_into()
            .expect("public key length");
        let signature_bytes: [u8; 64] = hex::decode(&first.signature)
            .expect("signature hex")
            .try_into()
            .expect("signature length");
        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes).expect("verifying key");
        let signature = Signature::from_bytes(&signature_bytes);
        verifying_key
            .verify(&signing_message(transaction), &signature)
            .expect("canonical transaction signature");
        assert!(transaction.inputs.iter().all(|input| {
            input.public_key == first.public_key && input.signature == first.signature
        }));
    }

    fn cleanup(dir: PathBuf, file: WalletKeystoreFile, session: WalletSession) {
        drop(session);
        drop(file);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn v1_session_signs_reviewed_plan_without_mutating_or_serializing_secret() {
        let (dir, file, address) = v1_fixture("v1");
        let mut session = WalletSession::new(policy(Duration::from_secs(5))).expect("session");
        session
            .unlock(&file, &SecretString::new(PASSWORD))
            .expect("unlock");
        let plan = transaction_plan(&address, CHAIN_ID);
        let unsigned_before = serde_json::to_string(&plan).expect("serialize unsigned plan");
        let signed = session
            .sign_transaction_plan(&plan, WalletPlanSigner::LegacyV1)
            .expect("sign v1 plan");
        assert_eq!(
            unsigned_before,
            serde_json::to_string(&plan).expect("serialize unchanged plan")
        );
        verify_signed_transaction(&signed.transaction);
        let encoded = serde_json::to_string(&signed).expect("serialize signed result");
        assert!(!encoded.contains(PASSWORD));
        assert!(!encoded.contains(&"55".repeat(ED25519_SECRET_KEY_BYTES)));
        cleanup(dir, file, session);
    }

    #[test]
    fn v2_session_signs_only_the_selected_deterministic_child() {
        let (dir, file, anchor) = seed_fixture("v2");
        let mut session = WalletSession::new(policy(Duration::from_secs(5))).expect("session");
        session
            .unlock(&file, &SecretString::new(PASSWORD))
            .expect("unlock");
        let plan = transaction_plan(&anchor, CHAIN_ID);
        let signed = session
            .sign_transaction_plan(
                &plan,
                WalletPlanSigner::DeterministicV2 {
                    account: 0,
                    branch: WalletDerivationBranch::Receive,
                    index: 0,
                },
            )
            .expect("sign deterministic plan");
        verify_signed_transaction(&signed.transaction);
        let encoded = serde_json::to_string(&signed).expect("serialize signed result");
        assert!(!encoded.contains(MNEMONIC));
        assert!(!encoded.contains(PASSWORD));

        assert!(matches!(
            session.sign_transaction_plan(
                &plan,
                WalletPlanSigner::DeterministicV2 {
                    account: 0,
                    branch: WalletDerivationBranch::Receive,
                    index: 1,
                },
            ),
            Err(WalletPlanSigningError::Plan(
                WalletPlanError::PublicKeyAddressMismatch { .. }
            ))
        ));
        assert!(matches!(
            session.sign_transaction_plan(&plan, WalletPlanSigner::LegacyV1),
            Err(WalletPlanSigningError::Session(
                WalletSessionError::WrongSecretKind
            ))
        ));
        cleanup(dir, file, session);
    }

    #[test]
    fn activated_v2_high_level_plan_signs_chain_bound_bytes_and_txid() {
        let (dir, file, anchor) = seed_fixture("activated-v2");
        let mut session = WalletSession::new(policy(Duration::from_secs(5))).expect("session");
        session
            .unlock(&file, &SecretString::new(PASSWORD))
            .expect("unlock");

        let identity = ProtocolActivationIdentity::activated_v2(
            CHAIN_ID,
            "wallet-signing-domain-test-genesis",
            "ghostdag-order-v1",
        );
        let plan = transaction_plan(&anchor, CHAIN_ID)
            .bind_activated_v2_protocol(identity.clone())
            .expect("bind activated-v2 identity");
        assert_eq!(
            plan.transaction.version,
            pulsedag_core::TRANSACTION_VERSION_V2
        );

        let signed = session
            .sign_transaction_plan(
                &plan,
                WalletPlanSigner::DeterministicV2 {
                    account: 0,
                    branch: WalletDerivationBranch::Receive,
                    index: 0,
                },
            )
            .expect("sign activated-v2 plan");

        let binding = signed
            .protocol_binding_v2
            .as_ref()
            .expect("signed result preserves protocol binding");
        assert_eq!(binding.identity, identity);
        assert_eq!(
            binding.fingerprint,
            binding.identity.fingerprint().expect("fingerprint")
        );
        assert_eq!(
            signed.transaction.txid,
            compute_txid_v2(&signed.transaction, CHAIN_ID).expect("v2 txid")
        );

        let first = signed.transaction.inputs.first().expect("signed input");
        let public_key_bytes: [u8; 32] = hex::decode(&first.public_key)
            .expect("public key hex")
            .try_into()
            .expect("public key length");
        let signature_bytes: [u8; 64] = hex::decode(&first.signature)
            .expect("signature hex")
            .try_into()
            .expect("signature length");
        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes).expect("verifying key");
        let signature = Signature::from_bytes(&signature_bytes);
        let message = signing_message_v2(&signed.transaction, CHAIN_ID).expect("v2 signing bytes");
        verifying_key
            .verify(&message, &signature)
            .expect("chain-bound v2 signature");

        let wrong_domain =
            signing_message_v2(&signed.transaction, "pulsedag-wrong-chain").expect("wrong domain");
        assert!(verifying_key.verify(&wrong_domain, &signature).is_err());

        let authorization = signed
            .protocol_authorization_v1
            .as_ref()
            .expect("activated-v2 result must carry full-identity authorization");
        verify_wallet_protocol_authorization_v1(binding, &signed.transaction, authorization)
            .expect("full protocol identity authorization");

        let foreign_binding = WalletProtocolBindingV2::new(
            ProtocolActivationIdentity::activated_v2(
                CHAIN_ID,
                "same-chain-id-different-genesis",
                "ghostdag-order-v1",
            ),
        )
        .expect("foreign binding");
        let mut relabeled_authorization = authorization.clone();
        relabeled_authorization.protocol_fingerprint = foreign_binding.fingerprint.clone();
        assert!(
            verify_wallet_protocol_authorization_v1(
                &foreign_binding,
                &signed.transaction,
                &relabeled_authorization,
            )
            .is_err(),
            "same-chain-id binding substitution must invalidate the signed authorization"
        );

        cleanup(dir, file, session);
    }

    #[test]
    fn network_mismatch_locked_and_expired_sessions_fail_closed() {
        let (dir, file, address) = v1_fixture("fail-closed");
        let plan = transaction_plan(&address, CHAIN_ID);
        let locked = WalletSession::new(policy(Duration::from_secs(5))).expect("locked session");
        assert!(matches!(
            locked.sign_transaction_plan(&plan, WalletPlanSigner::LegacyV1),
            Err(WalletPlanSigningError::Session(WalletSessionError::Locked))
        ));
        drop(locked);

        let mut session = WalletSession::new(policy(Duration::from_secs(5))).expect("session");
        session
            .unlock(&file, &SecretString::new(PASSWORD))
            .expect("unlock");
        let wrong_network = transaction_plan(&address, "different-chain");
        assert!(matches!(
            session.sign_transaction_plan(&wrong_network, WalletPlanSigner::LegacyV1),
            Err(WalletPlanSigningError::Plan(
                WalletPlanError::NetworkMismatch { .. }
            ))
        ));
        assert!(session.lock());
        drop(session);

        let mut expiring =
            WalletSession::new(policy(Duration::from_millis(120))).expect("expiring session");
        expiring
            .unlock(&file, &SecretString::new(PASSWORD))
            .expect("unlock expiring session");
        thread::sleep(Duration::from_millis(180));
        assert!(matches!(
            expiring.sign_transaction_plan(&plan, WalletPlanSigner::LegacyV1),
            Err(WalletPlanSigningError::Session(WalletSessionError::Locked))
        ));
        cleanup(dir, file, expiring);
    }
}
