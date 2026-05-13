use thiserror::Error;

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
    #[error("invalid state root: supplied {supplied}, computed {computed}")]
    InvalidStateRoot { supplied: String, computed: String },
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
