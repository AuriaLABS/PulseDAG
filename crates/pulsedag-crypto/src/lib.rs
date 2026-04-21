use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{address_from_public_key, errors::PulseError, types::Address};

pub fn generate_keypair() -> (String, String, Address) {
    let signing_key = SigningKey::generate(&mut rand::rngs::OsRng);
    let verifying_key = signing_key.verifying_key();
    let private_key = hex::encode(signing_key.to_bytes());
    let public_key = hex::encode(verifying_key.to_bytes());
    let address = address_from_public_key(&public_key);
    (private_key, public_key, address)
}

pub fn sign_message(private_key_hex: &str, message: &[u8]) -> Result<String, PulseError> {
    let bytes = hex::decode(private_key_hex).map_err(|e| PulseError::Internal(format!("private key decode error: {e}")))?;
    let key_bytes: [u8; 32] = bytes.try_into().map_err(|_| PulseError::Internal("invalid private key length".into()))?;
    let signing_key = SigningKey::from_bytes(&key_bytes);
    Ok(hex::encode(signing_key.sign(message).to_bytes()))
}
