use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{address_from_public_key, compute_txid, signing_message, Transaction};
use std::{env, fs, process};

fn usage() -> ! {
    eprintln!("usage: task30-tx-helper address SEED_U8 | sign SEED_U8 BUILD_RESPONSE_JSON");
    process::exit(64);
}

fn key(seed: &str) -> SigningKey {
    let seed: u8 = seed.parse().unwrap_or_else(|_| {
        eprintln!("SEED_U8 must be an integer in 0..=255");
        process::exit(64);
    });
    SigningKey::from_bytes(&[seed; 32])
}

fn public_key_hex(key: &SigningKey) -> String {
    hex::encode(key.verifying_key().to_bytes())
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    match args.as_slice() {
        [_, cmd, seed] if cmd == "address" => {
            let key = key(seed);
            println!("{}", address_from_public_key(&public_key_hex(&key)));
        }
        [_, cmd, seed, build_path] if cmd == "sign" => {
            let key = key(seed);
            let public_key = public_key_hex(&key);
            let raw = fs::read_to_string(build_path).unwrap_or_else(|error| {
                eprintln!("failed reading build response {build_path}: {error}");
                process::exit(65);
            });
            let value: serde_json::Value = serde_json::from_str(&raw).unwrap_or_else(|error| {
                eprintln!("invalid build response JSON: {error}");
                process::exit(65);
            });
            if value.get("ok").and_then(|v| v.as_bool()) != Some(true) {
                eprintln!("tx/build did not return ok=true: {value}");
                process::exit(66);
            }
            let tx_value = value
                .get("data")
                .and_then(|v| v.get("transaction"))
                .cloned()
                .unwrap_or_else(|| {
                    eprintln!("tx/build response omitted data.transaction: {value}");
                    process::exit(66);
                });
            let mut tx: Transaction = serde_json::from_value(tx_value).unwrap_or_else(|error| {
                eprintln!("invalid transaction in tx/build response: {error}");
                process::exit(66);
            });
            if tx.version != 1 {
                eprintln!("Task30 legacy runtime signer expected transaction version 1, got {}", tx.version);
                process::exit(67);
            }
            if tx.inputs.is_empty() {
                eprintln!("tx/build returned transaction without inputs");
                process::exit(67);
            }
            for input in &mut tx.inputs {
                input.public_key = public_key.clone();
                input.signature.clear();
            }
            let message = signing_message(&tx);
            let signature = hex::encode(key.sign(&message).to_bytes());
            for input in &mut tx.inputs {
                input.signature = signature.clone();
            }
            tx.txid = compute_txid(&tx);
            println!("{}", serde_json::json!({"transaction": tx}));
        }
        _ => usage(),
    }
}
