use std::fmt;

use thiserror::Error;

use crate::types::Hash;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvalidStateRootClassification {
    StaleTemplate,
    TrueInvalid,
    UnknownContext,
}

impl InvalidStateRootClassification {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StaleTemplate => "stale_template",
            Self::TrueInvalid => "true_invalid",
            Self::UnknownContext => "unknown_context",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InvalidStateRootDiagnostics {
    pub block_hash: Hash,
    pub height: u64,
    pub parent_hashes: Vec<Hash>,
    pub supplied_state_root: String,
    pub computed_state_root: String,
    pub tx_count: usize,
    pub coinbase_miner_address: Option<String>,
    pub selected_tip: Option<Hash>,
    pub selected_tip_height: Option<u64>,
    pub current_tips: Vec<Hash>,
    pub stale_template: bool,
    pub unknown_context: bool,
    pub classification: InvalidStateRootClassification,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidStateRootError {
    pub diagnostics: Box<InvalidStateRootDiagnostics>,
}

impl InvalidStateRootError {
    pub fn new(diagnostics: InvalidStateRootDiagnostics) -> Self {
        Self {
            diagnostics: Box::new(diagnostics),
        }
    }

    pub fn supplied(&self) -> &str {
        &self.diagnostics.supplied_state_root
    }

    pub fn computed(&self) -> &str {
        &self.diagnostics.computed_state_root
    }
}

impl fmt::Display for InvalidStateRootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let diagnostics = &self.diagnostics;
        write!(
            f,
            "invalid state root: supplied {}, computed {}; classification={}; block_hash={}; height={}; parents={}; tx_count={}; coinbase_miner={}; selected_tip={}; current_tips={}; stale_template={}; unknown_context={}",
            diagnostics.supplied_state_root,
            diagnostics.computed_state_root,
            diagnostics.classification.as_str(),
            diagnostics.block_hash,
            diagnostics.height,
            diagnostics.parent_hashes.join(","),
            diagnostics.tx_count,
            diagnostics
                .coinbase_miner_address
                .as_deref()
                .unwrap_or("<none>"),
            diagnostics.selected_tip.as_deref().unwrap_or("<none>"),
            diagnostics.current_tips.join(","),
            diagnostics.stale_template,
            diagnostics.unknown_context
        )
    }
}

#[derive(Debug, Error)]
pub enum PulseError {
    #[error("invalid signature")]
    InvalidSignature,
    #[error("insufficient funds")]
    InsufficientFunds,
    #[error("utxo not found")]
    UtxoNotFound,
    #[error("double spend")]
    DoubleSpend,
    #[error("transaction already exists")]
    TxAlreadyExists,
    #[error("block already exists")]
    BlockAlreadyExists,
    #[error("invalid block: {0}")]
    InvalidBlock(String),
    #[error("invalid transaction: {0}")]
    InvalidTransaction(String),
    #[error("invalid txid")]
    InvalidTxid,
    #[error("{0}")]
    InvalidStateRoot(Box<InvalidStateRootError>),
    #[error("missing coinbase transaction")]
    MissingCoinbase,
    #[error("multiple coinbase transactions")]
    MultipleCoinbase,
    #[error("coinbase transaction is not first")]
    CoinbaseNotFirst,
    #[error("excessive coinbase reward")]
    ExcessiveCoinbaseReward,
    #[error("duplicate UTXO outpoint: {0}")]
    DuplicateUtxoOutpoint(String),
    #[error("reward overflow")]
    RewardOverflow,
    #[error("duplicate UTXO outpoint: {0}")]
    DuplicateOutpoint(String),
    #[error("non-deterministic state: {0}")]
    NonDeterministicState(String),
    #[error("chain id mismatch")]
    ChainIdMismatch,
    #[error("storage error: {0}")]
    StorageError(String),
    #[error("p2p disabled")]
    P2pDisabled,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("internal error: {0}")]
    Internal(String),
}
