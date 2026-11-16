//! Compact, daemon-authorized GetBlock reply without cloning an entire DAG.
//!
//! The only constructor derives a block exclusively from ChainState's
//! currently accepted, same-chain DAG and its exact canonical hash key. This
//! intentionally excludes orphan, pending, staging and unverified disk data.
//! A pruned/missing hash is represented as an explicit authenticated miss.
//!
//! A future daemon-to-Swarm handoff can move this immutable owned snapshot
//! over a Tokio message and consume exactly one Noise response channel.
//! The dedicated production inbound protocol remains disabled until reviewed.

use pulsedag_core::state::ChainState;
use pulsedag_core::types::Block;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectGetBlockAcceptedSnapshotErrorV1 {
    ChainDomainMismatch,
    InvalidCanonicalHash,
}

#[derive(Debug, Clone)]
pub struct DirectGetBlockAcceptedSnapshotV1 {
    chain_id: String,
    requested_hash: String,
    block: Option<Block>,
}

impl DirectGetBlockAcceptedSnapshotV1 {
    pub fn from_accepted_chain(
        accepted_chain: &ChainState,
        configured_chain_id: &str,
        requested_hash: &str,
    ) -> Result<Self, DirectGetBlockAcceptedSnapshotErrorV1> {
        if configured_chain_id.is_empty()
            || configured_chain_id.len() > 96
            || accepted_chain.chain_id != configured_chain_id
        {
            return Err(DirectGetBlockAcceptedSnapshotErrorV1::ChainDomainMismatch);
        }
        if requested_hash.len() != 64
            || !requested_hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(DirectGetBlockAcceptedSnapshotErrorV1::InvalidCanonicalHash);
        }
        let block = accepted_chain
            .dag
            .blocks
            .get(requested_hash)
            .filter(|block| block.hash == requested_hash)
            .cloned();
        Ok(Self {
            chain_id: configured_chain_id.to_owned(),
            requested_hash: requested_hash.to_owned(),
            block,
        })
    }

    pub fn chain_id(&self) -> &str {
        &self.chain_id
    }

    pub fn requested_hash(&self) -> &str {
        &self.requested_hash
    }

    pub fn block(&self) -> Option<&Block> {
        self.block.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::genesis::init_chain_state;

    const CHAIN: &str = "accepted-snapshot-test";

    #[test]
    fn only_exact_accepted_genesis_is_available() {
        let chain = init_chain_state(CHAIN.to_string());
        let genesis = chain.dag.genesis_hash.clone();
        let approved =
            DirectGetBlockAcceptedSnapshotV1::from_accepted_chain(&chain, CHAIN, &genesis).unwrap();
        assert_eq!(approved.chain_id(), CHAIN);
        assert_eq!(approved.requested_hash(), genesis);
        assert_eq!(
            approved.block().map(|block| block.hash.as_str()),
            Some(genesis.as_str())
        );
    }

    #[test]
    fn orphan_and_inconsistent_dag_map_are_never_reply_authority() {
        let mut chain = init_chain_state(CHAIN.to_string());
        let mut orphan = chain
            .dag
            .blocks
            .get(&chain.dag.genesis_hash)
            .unwrap()
            .clone();
        orphan.hash = "b".repeat(64);
        chain
            .orphan_blocks
            .insert(orphan.hash.clone(), orphan.clone());
        let missing =
            DirectGetBlockAcceptedSnapshotV1::from_accepted_chain(&chain, CHAIN, &orphan.hash)
                .unwrap();
        assert!(missing.block().is_none());
        let fake_key = "c".repeat(64);
        chain.dag.blocks.insert(fake_key.clone(), orphan);
        let untrusted =
            DirectGetBlockAcceptedSnapshotV1::from_accepted_chain(&chain, CHAIN, &fake_key)
                .unwrap();
        assert!(untrusted.block().is_none());
    }

    #[test]
    fn wrong_chain_and_noncanonical_hash_fail_closed() {
        let mut chain = init_chain_state(CHAIN.to_string());
        let genesis = chain.dag.genesis_hash.clone();
        assert!(matches!(
            DirectGetBlockAcceptedSnapshotV1::from_accepted_chain(&chain, "foreign", &genesis),
            Err(DirectGetBlockAcceptedSnapshotErrorV1::ChainDomainMismatch)
        ));
        chain.chain_id = "foreign".to_string();
        assert!(matches!(
            DirectGetBlockAcceptedSnapshotV1::from_accepted_chain(&chain, CHAIN, &genesis),
            Err(DirectGetBlockAcceptedSnapshotErrorV1::ChainDomainMismatch)
        ));
        assert!(matches!(
            DirectGetBlockAcceptedSnapshotV1::from_accepted_chain(
                &chain,
                "foreign",
                &"B".repeat(64)
            ),
            Err(DirectGetBlockAcceptedSnapshotErrorV1::InvalidCanonicalHash)
        ));
    }
}
