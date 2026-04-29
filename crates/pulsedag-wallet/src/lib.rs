use serde::{Deserialize, Serialize};

use pulsedag_core::{
    compute_txid,
    errors::PulseError,
    signing_message,
    types::{Address, OutPoint, Transaction, TxInput, TxOutput, Utxo},
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

pub fn build_transaction(
    from: &str,
    to: &str,
    amount: u64,
    fee: u64,
    available_utxos: &[Utxo],
    nonce: u64,
) -> Result<BuildTxResponse, PulseError> {
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
    let mut tx = Transaction {
        txid: String::new(),
        version: 1,
        inputs,
        outputs,
        fee,
        nonce,
    };
    let message = signing_message(&tx);
    tx.txid = compute_txid(&tx);
    Ok(BuildTxResponse {
        transaction: tx,
        selected_utxos: selected
            .iter()
            .map(|u| SelectedUtxo {
                outpoint: u.outpoint.clone(),
                amount: u.amount,
            })
            .collect(),
        total_input,
        change,
        signing_message: hex::encode(message),
    })
}
