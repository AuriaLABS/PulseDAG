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
