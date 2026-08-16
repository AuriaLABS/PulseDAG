use serde::{Deserialize, Serialize};

use pulsedag_core::{
    compute_txid, compute_txid_v2,
    errors::PulseError,
    signing_message, signing_message_v2,
    types::{Address, OutPoint, Transaction, TxInput, TxOutput, Utxo},
    TRANSACTION_VERSION_V2,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildTxRequest {
    pub from: Address,
    pub to: Address,
    pub amount: u64,
    pub fee: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedUtxo {
    pub outpoint: OutPoint,
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildTxResponse {
    pub transaction: Transaction,
    pub selected_utxos: Vec<SelectedUtxo>,
    pub total_input: u64,
    pub change: u64,
    pub signing_message: String,
}

pub fn select_utxos(utxos: &[Utxo], target: u64) -> Result<(Vec<Utxo>, u64), PulseError> {
    let mut selected = Vec::new();
    let mut total = 0_u64;
    for utxo in utxos {
        selected.push(utxo.clone());
        total = total.saturating_add(utxo.amount);
        if total >= target {
            return Ok((selected, total));
        }
    }
    Err(PulseError::InsufficientFunds)
}

fn build_transaction_body(
    from: &str,
    to: &str,
    amount: u64,
    fee: u64,
    available_utxos: &[Utxo],
    nonce: u64,
    version: u32,
) -> Result<(Transaction, Vec<Utxo>, u64, u64), PulseError> {
    let target = amount
        .checked_add(fee)
        .ok_or_else(|| PulseError::InvalidTransaction("amount overflow".into()))?;
    let (selected, total_input) = select_utxos(available_utxos, target)?;
    let change = total_input - target;
    let inputs = selected
        .iter()
        .map(|u| TxInput {
            previous_output: u.outpoint.clone(),
            public_key: String::new(),
            signature: String::new(),
        })
        .collect::<Vec<_>>();
    let mut outputs = vec![TxOutput {
        address: to.to_string(),
        amount,
    }];
    if change > 0 {
        outputs.push(TxOutput {
            address: from.to_string(),
            amount: change,
        });
    }

    Ok((
        Transaction {
            txid: String::new(),
            version,
            inputs,
            outputs,
            fee,
            nonce,
        },
        selected,
        total_input,
        change,
    ))
}

fn build_response(
    transaction: Transaction,
    selected: &[Utxo],
    total_input: u64,
    change: u64,
    signing_message: Vec<u8>,
) -> BuildTxResponse {
    BuildTxResponse {
        transaction,
        selected_utxos: selected
            .iter()
            .map(|u| SelectedUtxo {
                outpoint: u.outpoint.clone(),
                amount: u.amount,
            })
            .collect(),
        total_input,
        change,
        signing_message: hex::encode(signing_message),
    }
}

/// Frozen legacy wallet builder. This remains transaction v1 and intentionally
/// does not gain chain binding.
pub fn build_transaction(
    from: &str,
    to: &str,
    amount: u64,
    fee: u64,
    available_utxos: &[Utxo],
    nonce: u64,
) -> Result<BuildTxResponse, PulseError> {
    let (mut tx, selected, total_input, change) =
        build_transaction_body(from, to, amount, fee, available_utxos, nonce, 1)?;
    let message = signing_message(&tx);
    tx.txid = compute_txid(&tx);
    Ok(build_response(tx, &selected, total_input, change, message))
}

/// Explicit chain-bound v2 wallet builder.
///
/// This API is non-activating: callers must choose it deliberately and provide
/// the exact canonical chain id. Existing callers of `build_transaction` remain
/// on the frozen v1 path until the v2.4 activation gate is wired.
pub fn build_transaction_v2(
    chain_id: &str,
    from: &str,
    to: &str,
    amount: u64,
    fee: u64,
    available_utxos: &[Utxo],
    nonce: u64,
) -> Result<BuildTxResponse, PulseError> {
    let (mut tx, selected, total_input, change) = build_transaction_body(
        from,
        to,
        amount,
        fee,
        available_utxos,
        nonce,
        TRANSACTION_VERSION_V2,
    )?;
    let message = signing_message_v2(&tx, chain_id)?;
    tx.txid = compute_txid_v2(&tx, chain_id)?;
    Ok(build_response(tx, &selected, total_input, change, message))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utxo(txid: &str, index: u32, amount: u64) -> Utxo {
        Utxo {
            outpoint: OutPoint {
                txid: txid.to_string(),
                index,
            },
            address: "pulse1source".to_string(),
            amount,
            coinbase: false,
            height: 1,
        }
    }

    #[test]
    fn legacy_builder_remains_v1() {
        let built = build_transaction(
            "pulse1source",
            "pulse1recipient",
            7,
            1,
            &[utxo("funding", 0, 10)],
            9,
        )
        .unwrap();

        assert_eq!(built.transaction.version, 1);
        assert_eq!(built.transaction.txid, compute_txid(&built.transaction));
    }

    #[test]
    fn v2_builder_is_chain_bound() {
        let available = [utxo("funding", 0, 10)];
        let testnet = build_transaction_v2(
            "pulsedag-testnet",
            "pulse1source",
            "pulse1recipient",
            7,
            1,
            &available,
            9,
        )
        .unwrap();
        let private = build_transaction_v2(
            "pulsedag-private",
            "pulse1source",
            "pulse1recipient",
            7,
            1,
            &available,
            9,
        )
        .unwrap();

        assert_eq!(testnet.transaction.version, TRANSACTION_VERSION_V2);
        assert_ne!(testnet.transaction.txid, private.transaction.txid);
        assert_ne!(testnet.signing_message, private.signing_message);
        assert_eq!(
            testnet.transaction.txid,
            compute_txid_v2(&testnet.transaction, "pulsedag-testnet").unwrap()
        );
    }

    #[test]
    fn v2_builder_rejects_empty_chain_id() {
        let err = build_transaction_v2(
            "",
            "pulse1source",
            "pulse1recipient",
            7,
            1,
            &[utxo("funding", 0, 10)],
            9,
        )
        .unwrap_err();

        assert!(matches!(err, PulseError::ChainIdMismatch));
    }
}
