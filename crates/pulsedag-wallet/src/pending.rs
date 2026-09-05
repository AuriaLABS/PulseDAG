use std::{error::Error, fmt};

use pulsedag_core::types::OutPoint;
use serde::{Deserialize, Serialize};

use crate::{
    plan::{WalletNetworkIdentity, WalletPlanError, WalletTransactionIntent},
    SelectedUtxo,
};

pub const WALLET_PENDING_JOURNAL_FORMAT: &str = "pulsedag-wallet-pending-journal";
pub const WALLET_PENDING_JOURNAL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WalletPendingState {
    Signed,
    SubmissionOutcomeUnknown,
    RelayAccepted,
    ObservedMempool,
    RelayRejected,
    Confirmed,
}

impl WalletPendingState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Signed => "signed",
            Self::SubmissionOutcomeUnknown => "submission_outcome_unknown",
            Self::RelayAccepted => "relay_accepted",
            Self::ObservedMempool => "observed_mempool",
            Self::RelayRejected => "relay_rejected",
            Self::Confirmed => "confirmed",
        }
    }

    pub const fn reserves_outpoints(self) -> bool {
        !matches!(self, Self::Confirmed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletPendingTransaction {
    pub final_txid: String,
    pub from: String,
    pub selected_outpoints: Vec<OutPoint>,
    pub state: WalletPendingState,
    pub rejection_code: Option<String>,
    pub rejection_message: Option<String>,
}

impl WalletPendingTransaction {
    fn validate(&self) -> Result<(), WalletPendingError> {
        validate_txid(&self.final_txid)?;
        validate_wallet_address(&self.from)?;
        validate_selected_outpoints(&self.selected_outpoints)?;
        match self.state {
            WalletPendingState::RelayRejected => {
                validate_text("rejection_code", self.rejection_code.as_deref())?;
                validate_text("rejection_message", self.rejection_message.as_deref())?;
            }
            _ if self.rejection_code.is_some() || self.rejection_message.is_some() => {
                return Err(WalletPendingError::InvalidField {
                    field: "rejection",
                    reason: "rejection metadata is allowed only for relay_rejected state",
                });
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WalletPendingJournal {
    pub format: String,
    pub version: u32,
    pub network: WalletNetworkIdentity,
    pub entries: Vec<WalletPendingTransaction>,
}

impl WalletPendingJournal {
    pub fn new(network: WalletNetworkIdentity) -> Result<Self, WalletPendingError> {
        network.validate().map_err(WalletPendingError::Plan)?;
        Ok(Self {
            format: WALLET_PENDING_JOURNAL_FORMAT.to_string(),
            version: WALLET_PENDING_JOURNAL_VERSION,
            network,
            entries: Vec::new(),
        })
    }

    pub fn validate(&self) -> Result<(), WalletPendingError> {
        if self.format != WALLET_PENDING_JOURNAL_FORMAT {
            return Err(WalletPendingError::InvalidField {
                field: "format",
                reason: "unsupported pending-journal format",
            });
        }
        if self.version != WALLET_PENDING_JOURNAL_VERSION {
            return Err(WalletPendingError::UnsupportedVersion(self.version));
        }
        self.network.validate().map_err(WalletPendingError::Plan)?;

        for (index, entry) in self.entries.iter().enumerate() {
            entry.validate()?;
            if self.entries[..index]
                .iter()
                .any(|prior| prior.final_txid == entry.final_txid)
            {
                return Err(WalletPendingError::DuplicateTransaction(
                    entry.final_txid.clone(),
                ));
            }
        }

        for (left_index, left) in self.entries.iter().enumerate() {
            if !left.state.reserves_outpoints() {
                continue;
            }
            for right in self.entries.iter().skip(left_index + 1) {
                if !right.state.reserves_outpoints() {
                    continue;
                }
                if left.selected_outpoints.iter().any(|left_outpoint| {
                    right
                        .selected_outpoints
                        .iter()
                        .any(|right_outpoint| right_outpoint == left_outpoint)
                }) {
                    return Err(WalletPendingError::ConflictingReservations);
                }
            }
        }
        Ok(())
    }

    pub fn ensure_network(
        &self,
        observed: &WalletNetworkIdentity,
    ) -> Result<(), WalletPendingError> {
        self.validate()?;
        self.network
            .ensure_matches(observed)
            .map_err(WalletPendingError::Plan)
    }

    /// Add one signed transaction reservation. Repeating the exact same
    /// reservation is idempotent and returns `Ok(false)`; a new reservation
    /// returns `Ok(true)`.
    pub fn reserve_signed(
        &mut self,
        final_txid: impl Into<String>,
        from: impl Into<String>,
        selected_utxos: &[SelectedUtxo],
    ) -> Result<bool, WalletPendingError> {
        self.validate()?;
        let final_txid = final_txid.into();
        let from = from.into();
        validate_txid(&final_txid)?;
        validate_wallet_address(&from)?;
        let selected_outpoints = selected_utxos
            .iter()
            .map(|selected| selected.outpoint.clone())
            .collect::<Vec<_>>();
        validate_selected_outpoints(&selected_outpoints)?;

        if let Some(existing) = self
            .entries
            .iter()
            .find(|entry| entry.final_txid == final_txid)
        {
            if existing.from == from && existing.selected_outpoints == selected_outpoints {
                return Ok(false);
            }
            return Err(WalletPendingError::TransactionIdentityMismatch(final_txid));
        }

        if self.entries.iter().any(|entry| {
            entry.state.reserves_outpoints()
                && entry.selected_outpoints.iter().any(|reserved| {
                    selected_outpoints
                        .iter()
                        .any(|selected| selected == reserved)
                })
        }) {
            return Err(WalletPendingError::ConflictingReservations);
        }

        self.entries.push(WalletPendingTransaction {
            final_txid,
            from,
            selected_outpoints,
            state: WalletPendingState::Signed,
            rejection_code: None,
            rejection_message: None,
        });
        self.validate()?;
        Ok(true)
    }

    pub fn ensure_selected_unreserved(
        &self,
        selected_utxos: &[SelectedUtxo],
    ) -> Result<(), WalletPendingError> {
        self.validate()?;
        for selected in selected_utxos {
            if self.entries.iter().any(|entry| {
                entry.state.reserves_outpoints()
                    && entry
                        .selected_outpoints
                        .iter()
                        .any(|reserved| reserved == &selected.outpoint)
            }) {
                return Err(WalletPendingError::ReservedOutpoint {
                    txid: selected.outpoint.txid.clone(),
                    index: selected.outpoint.index,
                });
            }
        }
        Ok(())
    }

    pub fn mark_submission_outcome_unknown(
        &mut self,
        final_txid: &str,
    ) -> Result<(), WalletPendingError> {
        self.transition(
            final_txid,
            WalletPendingState::SubmissionOutcomeUnknown,
            None,
        )
    }

    pub fn mark_relay_accepted(&mut self, final_txid: &str) -> Result<(), WalletPendingError> {
        self.transition(final_txid, WalletPendingState::RelayAccepted, None)
    }

    pub fn mark_observed_mempool(&mut self, final_txid: &str) -> Result<(), WalletPendingError> {
        self.transition(final_txid, WalletPendingState::ObservedMempool, None)
    }

    pub fn mark_confirmed(&mut self, final_txid: &str) -> Result<(), WalletPendingError> {
        self.transition(final_txid, WalletPendingState::Confirmed, None)
    }

    /// A generic relay rejection is recorded for later reconciliation but is
    /// not terminal spend evidence under the current public API, so selected
    /// outpoints remain reserved. After an ambiguous/accepted observation a
    /// later rejection cannot prove that an earlier submission failed.
    pub fn mark_relay_rejected(
        &mut self,
        final_txid: &str,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Result<(), WalletPendingError> {
        let code = code.into();
        let message = message.into();
        validate_text("rejection_code", Some(&code))?;
        validate_text("rejection_message", Some(&message))?;
        self.transition(
            final_txid,
            WalletPendingState::RelayRejected,
            Some((code, message)),
        )
    }

    pub fn entry(&self, final_txid: &str) -> Option<&WalletPendingTransaction> {
        self.entries
            .iter()
            .find(|entry| entry.final_txid == final_txid)
    }

    pub fn reserved_outpoints(&self) -> Vec<OutPoint> {
        self.entries
            .iter()
            .filter(|entry| entry.state.reserves_outpoints())
            .flat_map(|entry| entry.selected_outpoints.iter().cloned())
            .collect()
    }

    fn transition(
        &mut self,
        final_txid: &str,
        next: WalletPendingState,
        rejection: Option<(String, String)>,
    ) -> Result<(), WalletPendingError> {
        self.validate()?;
        validate_txid(final_txid)?;
        let entry = self
            .entries
            .iter_mut()
            .find(|entry| entry.final_txid == final_txid)
            .ok_or_else(|| WalletPendingError::UnknownTransaction(final_txid.to_string()))?;
        let current = entry.state;
        if !transition_allowed(current, next) {
            return Err(WalletPendingError::InvalidTransition { current, next });
        }

        entry.state = next;
        match rejection {
            Some((code, message)) => {
                entry.rejection_code = Some(code);
                entry.rejection_message = Some(message);
            }
            None => {
                entry.rejection_code = None;
                entry.rejection_message = None;
            }
        }
        self.validate()
    }
}

#[derive(Debug)]
pub enum WalletPendingError {
    InvalidField {
        field: &'static str,
        reason: &'static str,
    },
    UnsupportedVersion(u32),
    Plan(WalletPlanError),
    DuplicateTransaction(String),
    DuplicateOutpoint,
    ConflictingReservations,
    ReservedOutpoint {
        txid: String,
        index: u32,
    },
    TransactionIdentityMismatch(String),
    UnknownTransaction(String),
    InvalidTransition {
        current: WalletPendingState,
        next: WalletPendingState,
    },
}

impl fmt::Display for WalletPendingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField { field, reason } => {
                write!(f, "invalid wallet pending field {field}: {reason}")
            }
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported wallet pending journal version: {version}")
            }
            Self::Plan(error) => write!(f, "wallet pending validation failed: {error}"),
            Self::DuplicateTransaction(txid) => {
                write!(f, "wallet pending journal contains duplicate txid: {txid}")
            }
            Self::DuplicateOutpoint => {
                f.write_str("wallet pending transaction contains duplicate selected outpoint")
            }
            Self::ConflictingReservations => {
                f.write_str("wallet pending journal contains conflicting active UTXO reservations")
            }
            Self::ReservedOutpoint { txid, index } => write!(
                f,
                "wallet UTXO is reserved by a pending transaction: {txid}:{index}"
            ),
            Self::TransactionIdentityMismatch(txid) => write!(
                f,
                "wallet pending txid already exists with different reservation metadata: {txid}"
            ),
            Self::UnknownTransaction(txid) => {
                write!(f, "wallet pending transaction is unknown: {txid}")
            }
            Self::InvalidTransition { current, next } => write!(
                f,
                "invalid wallet pending transition: {} -> {}",
                current.as_str(),
                next.as_str()
            ),
        }
    }
}

impl Error for WalletPendingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Plan(error) => Some(error),
            _ => None,
        }
    }
}

fn transition_allowed(current: WalletPendingState, next: WalletPendingState) -> bool {
    if current == next {
        return true;
    }
    match current {
        WalletPendingState::Signed => matches!(
            next,
            WalletPendingState::SubmissionOutcomeUnknown
                | WalletPendingState::RelayAccepted
                | WalletPendingState::ObservedMempool
                | WalletPendingState::RelayRejected
                | WalletPendingState::Confirmed
        ),
        WalletPendingState::SubmissionOutcomeUnknown => matches!(
            next,
            WalletPendingState::RelayAccepted
                | WalletPendingState::ObservedMempool
                | WalletPendingState::Confirmed
        ),
        WalletPendingState::RelayAccepted => matches!(
            next,
            WalletPendingState::ObservedMempool | WalletPendingState::Confirmed
        ),
        WalletPendingState::ObservedMempool => matches!(next, WalletPendingState::Confirmed),
        WalletPendingState::RelayRejected => matches!(
            next,
            WalletPendingState::ObservedMempool | WalletPendingState::Confirmed
        ),
        WalletPendingState::Confirmed => false,
    }
}

fn validate_wallet_address(value: &str) -> Result<(), WalletPendingError> {
    WalletTransactionIntent::new(value, value, 1, 0)
        .map(|_| ())
        .map_err(WalletPendingError::Plan)
}

fn validate_txid(value: &str) -> Result<(), WalletPendingError> {
    let decoded = hex::decode(value).map_err(|_| WalletPendingError::InvalidField {
        field: "final_txid",
        reason: "must be hexadecimal",
    })?;
    if decoded.len() != 32 {
        return Err(WalletPendingError::InvalidField {
            field: "final_txid",
            reason: "must encode exactly 32 bytes",
        });
    }
    if hex::encode(decoded) != value {
        return Err(WalletPendingError::InvalidField {
            field: "final_txid",
            reason: "must use canonical lowercase hexadecimal encoding",
        });
    }
    Ok(())
}

fn validate_selected_outpoints(outpoints: &[OutPoint]) -> Result<(), WalletPendingError> {
    if outpoints.is_empty() {
        return Err(WalletPendingError::InvalidField {
            field: "selected_outpoints",
            reason: "must contain at least one outpoint",
        });
    }
    for (index, outpoint) in outpoints.iter().enumerate() {
        validate_txid_component(&outpoint.txid)?;
        if outpoints[..index].iter().any(|prior| prior == outpoint) {
            return Err(WalletPendingError::DuplicateOutpoint);
        }
    }
    Ok(())
}

fn validate_txid_component(value: &str) -> Result<(), WalletPendingError> {
    let decoded = hex::decode(value).map_err(|_| WalletPendingError::InvalidField {
        field: "selected_outpoints.txid",
        reason: "must be hexadecimal",
    })?;
    if decoded.len() != 32 || hex::encode(decoded) != value {
        return Err(WalletPendingError::InvalidField {
            field: "selected_outpoints.txid",
            reason: "must be canonical lowercase 32-byte hexadecimal",
        });
    }
    Ok(())
}

fn validate_text(field: &'static str, value: Option<&str>) -> Result<(), WalletPendingError> {
    let value = value.ok_or(WalletPendingError::InvalidField {
        field,
        reason: "must be present",
    })?;
    if value.is_empty() || value.trim() != value {
        return Err(WalletPendingError::InvalidField {
            field,
            reason: "must be non-empty without leading or trailing whitespace",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use pulsedag_core::{address_from_public_key, types::OutPoint};

    use super::*;

    fn network(chain_id: &str) -> WalletNetworkIdentity {
        WalletNetworkIdentity::new("public-testnet", chain_id).expect("network")
    }

    fn address() -> String {
        address_from_public_key(&"ab".repeat(32))
    }

    fn selected(txid_byte: &str, index: u32) -> SelectedUtxo {
        SelectedUtxo {
            outpoint: OutPoint {
                txid: txid_byte.repeat(32),
                index,
            },
            amount: 100,
        }
    }

    fn final_txid(byte: &str) -> String {
        byte.repeat(32)
    }

    #[test]
    fn exact_same_reservation_is_idempotent_and_conflicts_fail_closed() {
        let mut journal = WalletPendingJournal::new(network("chain-a")).expect("journal");
        let first = [selected("11", 0)];
        assert!(journal
            .reserve_signed(final_txid("aa"), address(), &first)
            .expect("reserve"));
        assert!(!journal
            .reserve_signed(final_txid("aa"), address(), &first)
            .expect("idempotent"));
        assert!(matches!(
            journal.reserve_signed(final_txid("bb"), address(), &first),
            Err(WalletPendingError::ConflictingReservations)
        ));
    }

    #[test]
    fn unknown_and_accepted_states_keep_reservations_until_evidence_is_terminal() {
        let mut journal = WalletPendingJournal::new(network("chain-a")).expect("journal");
        let selected = [selected("11", 0)];
        let txid = final_txid("aa");
        journal
            .reserve_signed(&txid, address(), &selected)
            .expect("reserve");
        journal
            .mark_submission_outcome_unknown(&txid)
            .expect("unknown");
        assert_eq!(journal.reserved_outpoints().len(), 1);
        assert!(journal
            .mark_relay_rejected(&txid, "TX_REJECTED", "later rejection")
            .is_err());
        assert_eq!(
            journal.entry(&txid).expect("entry").state,
            WalletPendingState::SubmissionOutcomeUnknown
        );
        journal.mark_relay_accepted(&txid).expect("accepted");
        journal.mark_observed_mempool(&txid).expect("mempool");
        assert_eq!(journal.reserved_outpoints().len(), 1);
        journal.mark_confirmed(&txid).expect("confirmed");
        assert!(journal.reserved_outpoints().is_empty());
    }

    #[test]
    fn generic_relay_rejection_keeps_reservation_until_stronger_evidence() {
        let mut journal = WalletPendingJournal::new(network("chain-a")).expect("journal");
        let selected = [selected("11", 0)];
        let txid = final_txid("aa");
        journal
            .reserve_signed(&txid, address(), &selected)
            .expect("reserve");
        journal
            .mark_relay_rejected(&txid, "TX_REJECTED", "generic relay rejection")
            .expect("reject observation");
        assert_eq!(journal.reserved_outpoints().len(), 1);
        assert_eq!(
            journal
                .entry(&txid)
                .expect("entry")
                .rejection_code
                .as_deref(),
            Some("TX_REJECTED")
        );
        journal.mark_observed_mempool(&txid).expect("mempool");
        assert_eq!(journal.reserved_outpoints().len(), 1);
        journal.mark_confirmed(&txid).expect("confirmed");
        assert!(journal.reserved_outpoints().is_empty());
    }

    #[test]
    fn network_mismatch_and_malformed_journal_fail_closed() {
        let mut journal = WalletPendingJournal::new(network("chain-a")).expect("journal");
        journal
            .reserve_signed(final_txid("aa"), address(), &[selected("11", 0)])
            .expect("reserve");
        assert!(journal.ensure_network(&network("chain-b")).is_err());

        let mut malformed = journal.clone();
        malformed.entries[0].final_txid = "AA".repeat(32);
        assert!(malformed.validate().is_err());

        let mut duplicate = journal.clone();
        duplicate.entries.push(duplicate.entries[0].clone());
        assert!(duplicate.validate().is_err());
    }

    #[test]
    fn reserved_outpoint_check_releases_only_after_confirmation() {
        let mut journal = WalletPendingJournal::new(network("chain-a")).expect("journal");
        let first = [selected("11", 0)];
        let txid = final_txid("aa");
        journal
            .reserve_signed(&txid, address(), &first)
            .expect("reserve");
        assert!(journal.ensure_selected_unreserved(&first).is_err());
        journal
            .mark_relay_rejected(&txid, "TX_REJECTED", "generic rejection")
            .expect("reject observation");
        assert!(journal.ensure_selected_unreserved(&first).is_err());
        journal.mark_confirmed(&txid).expect("confirmed");
        assert!(journal.ensure_selected_unreserved(&first).is_ok());
    }
}
