#[allow(dead_code)]
mod original {
    include!("pulsedag-wallet-base.inc");

    fn permission_policy_preflight() -> CliResult<()> {
        if WalletKeystoreFile::permission_policy_preflight()
            == pulsedag_wallet::WalletKeystorePermissionStatus::NotEnforcedOnThisPlatform
        {
            return Err(invalid_input(
                "restrictive keystore permissions are not enforced on this platform",
            )
            .into());
        }
        Ok(())
    }

    fn run_restore_phase1(args: RestoreArgs, secrets: RestoreSecrets) -> CliResult<RestoreOutput> {
        permission_policy_preflight()?;
        run_restore(args, secrets)
    }

    async fn run_phase1() -> CliResult<()> {
        match parse_command()? {
            Command::Restore(args) => {
                let secrets = read_restore_secrets_from_stdin()?;
                write_json(&run_restore_phase1(args, secrets)?)
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
            Command::Balance(args) => write_json(&run_balance(args).await?),
            Command::Utxos(args) => write_json(&run_utxos(args).await?),
            Command::FeeEstimate(args) => write_json(&run_fee_estimate(args).await?),
            Command::TxPreview(args) => {
                let password = read_password_from_stdin()?;
                write_json(&run_tx_preview(args, &password)?)
            }
            Command::TxSign(args) => {
                let password = read_password_from_stdin()?;
                write_json(&run_tx_sign(args, &password)?)
            }
            Command::TxBroadcast(args) => write_json(&run_tx_broadcast(args).await?),
        }
    }

    pub(super) async fn phase1_entry() {
        if let Err(error) = run_phase1().await {
            if let Some(encoded) = machine_readable_pending_error(error.as_ref()) {
                eprintln!("{encoded}");
            } else {
                eprintln!("pulsedag-wallet: {error}");
            }
            std::process::exit(1);
        }
    }

    #[cfg(test)]
    mod phase1_tests {
        use std::{
            fs,
            time::{SystemTime, UNIX_EPOCH},
        };

        use super::*;

        fn args(values: &[&str]) -> impl Iterator<Item = String> {
            values
                .iter()
                .map(|value| (*value).to_string())
                .collect::<Vec<_>>()
                .into_iter()
        }

        #[test]
        fn secret_canaries_are_not_reflected_in_parser_errors_or_public_restore_output() {
            let password_canary = "PR1169_PASSWORD_CANARY";
            let mnemonic_canary = "PR1169_MNEMONIC_CANARY";
            let passphrase_canary = "PR1169_BIP39_PASSPHRASE_CANARY";

            for (flag, canary) in [
                ("--password", password_canary),
                ("--mnemonic", mnemonic_canary),
                ("--bip39-passphrase", passphrase_canary),
            ] {
                let error = parse_command_from(args(&[
                    "restore",
                    "--keystore",
                    "wallet.json",
                    "--network-profile",
                    "public-testnet",
                    "--chain-id",
                    "pulsedag-public-testnet",
                    flag,
                    canary,
                ]))
                .expect_err("secret-bearing command-line option must be rejected")
                .to_string();
                assert!(!error.contains(canary));
            }

            let output = RestoreOutput {
                network_profile: "public-testnet".to_string(),
                chain_id: "pulsedag-public-testnet".to_string(),
                account: 0,
                anchor_address: "pulse1publicrestoreoutput".to_string(),
                keystore: "wallet.json".to_string(),
            };
            let encoded = serde_json::to_string(&output).expect("serialize public restore output");
            for canary in [password_canary, mnemonic_canary, passphrase_canary] {
                assert!(!encoded.contains(canary));
            }
        }

        #[cfg(not(unix))]
        #[test]
        fn restore_refuses_unenforced_private_permissions_before_publish() {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock after unix epoch")
                .as_nanos();
            let parent = std::env::temp_dir().join(format!(
                "pulsedag-wallet-pr1169-{}-{nonce}",
                std::process::id()
            ));
            let target = parent.join("wallet.json");
            let _ = fs::remove_dir_all(&parent);

            let password_canary = "PR1169_PASSWORD_CANARY";
            let mnemonic_canary = "PR1169_MNEMONIC_CANARY";
            let passphrase_canary = "PR1169_BIP39_PASSPHRASE_CANARY";
            let error = run_restore_phase1(
                RestoreArgs {
                    keystore: target.clone(),
                    network_profile: "public-testnet".to_string(),
                    chain_id: "pulsedag-public-testnet".to_string(),
                },
                RestoreSecrets {
                    password: SecretString::new(password_canary.to_string()),
                    mnemonic: SecretString::new(mnemonic_canary.to_string()),
                    bip39_passphrase: Some(SecretString::new(passphrase_canary.to_string())),
                },
            )
            .expect_err("restore must fail closed when restrictive permissions are unavailable")
            .to_string();

            assert!(error
                .contains("restrictive keystore permissions are not enforced on this platform"));
            for canary in [password_canary, mnemonic_canary, passphrase_canary] {
                assert!(!error.contains(canary));
            }
            assert!(!target.exists());
            assert!(!parent.exists());
        }
    }
}

#[tokio::main]
async fn main() {
    original::phase1_entry().await;
}
