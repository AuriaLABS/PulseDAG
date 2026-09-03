use std::collections::HashSet;

use pulsedag_core::{errors::PulseError, types::Utxo};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::SelectedUtxo;

pub const WALLET_FUNDING_SNAPSHOT_DOMAIN_V1: &str = "PulseDAG:wallet-funding-snapshot:v1";

/// Explicit acknowledgements for transaction shapes that are easy to trigger
/// accidentally. False is always the fail-closed value.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletSafetyAcknowledgements {
    pub self_send: bool,
    pub spend_all: bool,
}

impl WalletSafetyAcknowledgements {
    pub const fn none() -> Self {
        Self {
            self_send: false,
            spend_all: false,
        }
    }

    pub const fn new(self_send: bool, spend_all: bool) -> Self {
        Self {
            self_send,
            spend_all,
        }
    }
}

/// Compact, deterministic description of the complete UTXO set that was
/// available when a transaction plan was built. The commitment is review and
/// audit evidence; the count/amount fields let an offline signer distinguish a
/// genuine spend-all plan from an ordinary zero-change selection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletFundingSnapshot {
    pub utxo_count: usize,
    pub total_amount: u64,
    pub commitment_hex: String,
}

impl WalletFundingSnapshot {
    pub fn from_utxos(utxos: &[Utxo]) -> Result<Self, PulseError> {
        let mut ordered = utxos.iter().collect::<Vec<_>>();
        ordered.sort_by(|left, right| {
            left.outpoint
                .txid
                .cmp(&right.outpoint.txid)
                .then_with(|| left.outpoint.index.cmp(&right.outpoint.index))
                .then_with(|| left.amount.cmp(&right.amount))
        });

        let mut seen = HashSet::with_capacity(ordered.len());
        let mut total_amount = 0_u64;
        let mut hasher = Sha256::new();
        hasher.update(WALLET_FUNDING_SNAPSHOT_DOMAIN_V1.as_bytes());
        hasher.update((ordered.len() as u64).to_be_bytes());

        for utxo in ordered {
            if !seen.insert((utxo.outpoint.txid.as_str(), utxo.outpoint.index)) {
                return Err(PulseError::InvalidTransaction(
                    "duplicate UTXO outpoint in wallet funding snapshot".into(),
                ));
            }
            total_amount = total_amount.checked_add(utxo.amount).ok_or_else(|| {
                PulseError::InvalidTransaction("wallet funding snapshot amount overflow".into())
            })?;

            let txid = utxo.outpoint.txid.as_bytes();
            hasher.update((txid.len() as u64).to_be_bytes());
            hasher.update(txid);
            hasher.update(utxo.outpoint.index.to_be_bytes());
            hasher.update(utxo.amount.to_be_bytes());
        }

        Ok(Self {
            utxo_count: utxos.len(),
            total_amount,
            commitment_hex: hex::encode(hasher.finalize()),
        })
    }

    pub fn is_spend_all(
        &self,
        selected_utxos: &[SelectedUtxo],
        total_input: u64,
        change: u64,
    ) -> bool {
        change == 0
            && selected_utxos.len() == self.utxo_count
            && total_input == self.total_amount
    }

    pub fn validate_shape(
        &self,
        selected_utxos: &[SelectedUtxo],
        total_input: u64,
    ) -> Result<(), PulseError> {
        if selected_utxos.len() > self.utxo_count {
            return Err(PulseError::InvalidTransaction(
                "wallet plan selects more UTXOs than its funding snapshot".into(),
            ));
        }
        if total_input > self.total_amount {
            return Err(PulseError::InvalidTransaction(
                "wallet plan input total exceeds its funding snapshot".into(),
            ));
        }
        if self.commitment_hex.len() != 64
            || !self
                .commitment_hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(PulseError::InvalidTransaction(
                "wallet funding snapshot commitment is not canonical lowercase sha256 hex".into(),
            ));
        }
        Ok(())
    }
}

pub fn validate_wallet_safety_acknowledgements(
    acknowledgements: WalletSafetyAcknowledgements,
    from: &str,
    to: &str,
    snapshot: &WalletFundingSnapshot,
    selected_utxos: &[SelectedUtxo],
    total_input: u64,
    change: u64,
) -> Result<(), PulseError> {
    snapshot.validate_shape(selected_utxos, total_input)?;

    if from == to && !acknowledgements.self_send {
        return Err(PulseError::InvalidTransaction(
            "wallet self-send requires explicit acknowledgement".into(),
        ));
    }
    if snapshot.is_spend_all(selected_utxos, total_input, change)
        && !acknowledgements.spend_all
    {
        return Err(PulseError::InvalidTransaction(
            "wallet spend-all requires explicit acknowledgement".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use pulsedag_core::types::OutPoint;

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

    fn selected(txid: &str, index: u32, amount: u64) -> SelectedUtxo {
        SelectedUtxo {
            outpoint: OutPoint {
                txid: txid.to_string(),
                index,
            },
            amount,
        }
    }

    #[test]
    fn funding_snapshot_is_order_independent() {
        let first = vec![utxo("b", 0, 4), utxo("a", 1, 7)];
        let second = vec![utxo("a", 1, 7), utxo("b", 0, 4)];
        assert_eq!(
            WalletFundingSnapshot::from_utxos(&first).unwrap(),
            WalletFundingSnapshot::from_utxos(&second).unwrap()
        );
    }

    #[test]
    fn funding_snapshot_rejects_duplicate_outpoints() {
        let values = vec![utxo("a", 0, 4), utxo("a", 0, 4)];
        assert!(WalletFundingSnapshot::from_utxos(&values).is_err());
    }

    #[test]
    fn spend_all_requires_complete_snapshot_and_zero_change() {
        let snapshot = WalletFundingSnapshot::from_utxos(&[
            utxo("a", 0, 4),
            utxo("b", 0, 6),
        ])
        .unwrap();
        let all = vec![selected("a", 0, 4), selected("b", 0, 6)];
        assert!(snapshot.is_spend_all(&all, 10, 0));
        assert!(!snapshot.is_spend_all(&all, 10, 1));
        assert!(!snapshot.is_spend_all(&all[..1], 4, 0));
    }

    #[test]
    fn self_send_and_spend_all_fail_closed_without_acknowledgement() {
        let snapshot = WalletFundingSnapshot::from_utxos(&[utxo("a", 0, 10)]).unwrap();
        let selected = vec![selected("a", 0, 10)];

        assert!(validate_wallet_safety_acknowledgements(
            WalletSafetyAcknowledgements::none(),
            "pulse1source",
            "pulse1source",
            &snapshot,
            &selected,
            10,
            0,
        )
        .is_err());

        assert!(validate_wallet_safety_acknowledgements(
            WalletSafetyAcknowledgements::new(true, true),
            "pulse1source",
            "pulse1source",
            &snapshot,
            &selected,
            10,
            0,
        )
        .is_ok());
    }
}
