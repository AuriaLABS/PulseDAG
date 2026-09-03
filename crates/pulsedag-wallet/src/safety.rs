use pulsedag_core::{
    errors::PulseError,
    types::{OutPoint, Utxo},
};
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

/// One canonical entry in the complete funding set that was reviewed while a
/// transaction plan was built. Keeping the entries in the plan lets an offline
/// signer recompute the snapshot summary instead of trusting mutable metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletFundingEntry {
    pub outpoint: OutPoint,
    pub amount: u64,
}

/// Deterministic, self-verifying description of the complete UTXO set that was
/// available when a transaction plan was built. The entries are review metadata
/// only: they are not added to transaction canonicalization or signing bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletFundingSnapshot {
    pub entries: Vec<WalletFundingEntry>,
    pub utxo_count: usize,
    pub total_amount: u64,
    pub commitment_hex: String,
}

impl WalletFundingSnapshot {
    pub fn from_utxos(utxos: &[Utxo]) -> Result<Self, PulseError> {
        let entries = utxos
            .iter()
            .map(|utxo| WalletFundingEntry {
                outpoint: utxo.outpoint.clone(),
                amount: utxo.amount,
            })
            .collect::<Vec<_>>();
        Self::from_entries(entries)
    }

    fn from_entries(mut entries: Vec<WalletFundingEntry>) -> Result<Self, PulseError> {
        sort_entries(&mut entries);
        validate_unique_outpoints(&entries)?;
        let (total_amount, commitment_hex) = summarize_entries(&entries)?;
        Ok(Self {
            utxo_count: entries.len(),
            entries,
            total_amount,
            commitment_hex,
        })
    }

    pub fn validate_integrity(&self) -> Result<(), PulseError> {
        let mut canonical = self.entries.clone();
        sort_entries(&mut canonical);
        if canonical != self.entries {
            return Err(PulseError::InvalidTransaction(
                "wallet funding snapshot entries are not in canonical order".into(),
            ));
        }
        validate_unique_outpoints(&canonical)?;
        let (expected_total, expected_commitment) = summarize_entries(&canonical)?;
        if self.utxo_count != canonical.len() {
            return Err(PulseError::InvalidTransaction(
                "wallet funding snapshot count does not match entries".into(),
            ));
        }
        if self.total_amount != expected_total {
            return Err(PulseError::InvalidTransaction(
                "wallet funding snapshot total does not match entries".into(),
            ));
        }
        if self.commitment_hex != expected_commitment {
            return Err(PulseError::InvalidTransaction(
                "wallet funding snapshot commitment does not match entries".into(),
            ));
        }
        Ok(())
    }

    pub fn validate_selection(
        &self,
        selected_utxos: &[SelectedUtxo],
        total_input: u64,
    ) -> Result<(), PulseError> {
        self.validate_integrity()?;
        let selected = canonical_selected_entries(selected_utxos)?;
        let selected_total = selected.iter().try_fold(0_u64, |total, entry| {
            total.checked_add(entry.amount).ok_or_else(|| {
                PulseError::InvalidTransaction("wallet selected input amount overflow".into())
            })
        })?;
        if selected_total != total_input {
            return Err(PulseError::InvalidTransaction(
                "wallet selected input total does not match plan".into(),
            ));
        }
        for entry in &selected {
            if !self.entries.iter().any(|funding| funding == entry) {
                return Err(PulseError::InvalidTransaction(
                    "wallet plan selects an input absent from its funding snapshot".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn is_spend_all(
        &self,
        selected_utxos: &[SelectedUtxo],
        total_input: u64,
        change: u64,
    ) -> Result<bool, PulseError> {
        self.validate_selection(selected_utxos, total_input)?;
        if change != 0 || total_input != self.total_amount {
            return Ok(false);
        }
        let selected = canonical_selected_entries(selected_utxos)?;
        Ok(selected == self.entries)
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
    snapshot.validate_selection(selected_utxos, total_input)?;

    if from == to && !acknowledgements.self_send {
        return Err(PulseError::InvalidTransaction(
            "wallet self-send requires explicit acknowledgement".into(),
        ));
    }
    if snapshot.is_spend_all(selected_utxos, total_input, change)? && !acknowledgements.spend_all {
        return Err(PulseError::InvalidTransaction(
            "wallet spend-all requires explicit acknowledgement".into(),
        ));
    }
    Ok(())
}

fn canonical_selected_entries(
    selected_utxos: &[SelectedUtxo],
) -> Result<Vec<WalletFundingEntry>, PulseError> {
    let mut entries = selected_utxos
        .iter()
        .map(|selected| WalletFundingEntry {
            outpoint: selected.outpoint.clone(),
            amount: selected.amount,
        })
        .collect::<Vec<_>>();
    sort_entries(&mut entries);
    validate_unique_outpoints(&entries)?;
    Ok(entries)
}

fn sort_entries(entries: &mut [WalletFundingEntry]) {
    entries.sort_by(|left, right| {
        left.outpoint
            .txid
            .cmp(&right.outpoint.txid)
            .then_with(|| left.outpoint.index.cmp(&right.outpoint.index))
            .then_with(|| left.amount.cmp(&right.amount))
    });
}

fn validate_unique_outpoints(entries: &[WalletFundingEntry]) -> Result<(), PulseError> {
    for pair in entries.windows(2) {
        if pair[0].outpoint == pair[1].outpoint {
            return Err(PulseError::InvalidTransaction(
                "duplicate UTXO outpoint in wallet funding snapshot".into(),
            ));
        }
    }
    Ok(())
}

fn summarize_entries(entries: &[WalletFundingEntry]) -> Result<(u64, String), PulseError> {
    let mut total_amount = 0_u64;
    let mut hasher = Sha256::new();
    hasher.update(WALLET_FUNDING_SNAPSHOT_DOMAIN_V1.as_bytes());
    hasher.update((entries.len() as u64).to_be_bytes());

    for entry in entries {
        total_amount = total_amount.checked_add(entry.amount).ok_or_else(|| {
            PulseError::InvalidTransaction("wallet funding snapshot amount overflow".into())
        })?;
        let txid = entry.outpoint.txid.as_bytes();
        hasher.update((txid.len() as u64).to_be_bytes());
        hasher.update(txid);
        hasher.update(entry.outpoint.index.to_be_bytes());
        hasher.update(entry.amount.to_be_bytes());
    }

    Ok((total_amount, hex::encode(hasher.finalize())))
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
    fn funding_snapshot_is_order_independent_and_self_verifying() {
        let first = vec![utxo("b", 0, 4), utxo("a", 1, 7)];
        let second = vec![utxo("a", 1, 7), utxo("b", 0, 4)];
        let first = WalletFundingSnapshot::from_utxos(&first).unwrap();
        let second = WalletFundingSnapshot::from_utxos(&second).unwrap();
        assert_eq!(first, second);
        assert!(first.validate_integrity().is_ok());
    }

    #[test]
    fn funding_snapshot_rejects_duplicate_outpoints() {
        let values = vec![utxo("a", 0, 4), utxo("a", 0, 4)];
        assert!(WalletFundingSnapshot::from_utxos(&values).is_err());
    }

    #[test]
    fn snapshot_metadata_tampering_fails_integrity_validation() {
        let original =
            WalletFundingSnapshot::from_utxos(&[utxo("a", 0, 4), utxo("b", 0, 6)]).unwrap();

        let mut count = original.clone();
        count.utxo_count += 1;
        assert!(count.validate_integrity().is_err());

        let mut total = original.clone();
        total.total_amount += 1;
        assert!(total.validate_integrity().is_err());

        let mut commitment = original.clone();
        commitment.commitment_hex = "00".repeat(32);
        assert!(commitment.validate_integrity().is_err());

        let mut entry = original;
        entry.entries[0].amount += 1;
        assert!(entry.validate_integrity().is_err());
    }

    #[test]
    fn selection_must_belong_to_snapshot_and_match_total() {
        let snapshot = WalletFundingSnapshot::from_utxos(&[utxo("a", 0, 4)]).unwrap();
        assert!(snapshot
            .validate_selection(&[selected("a", 0, 4)], 4)
            .is_ok());
        assert!(snapshot
            .validate_selection(&[selected("outside", 0, 4)], 4)
            .is_err());
        assert!(snapshot
            .validate_selection(&[selected("a", 0, 4)], 5)
            .is_err());
    }

    #[test]
    fn spend_all_requires_exact_complete_snapshot_and_zero_change() {
        let snapshot =
            WalletFundingSnapshot::from_utxos(&[utxo("a", 0, 4), utxo("b", 0, 6)]).unwrap();
        let all = vec![selected("b", 0, 6), selected("a", 0, 4)];
        assert!(snapshot.is_spend_all(&all, 10, 0).unwrap());
        assert!(!snapshot.is_spend_all(&all, 10, 1).unwrap());
        assert!(!snapshot.is_spend_all(&[selected("a", 0, 4)], 4, 0).unwrap());
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
