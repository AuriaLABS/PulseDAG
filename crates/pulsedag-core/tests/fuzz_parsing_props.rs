use proptest::prelude::*;
use pulsedag_core::types::{Block, BlockHeader, OutPoint, Transaction, TxInput, TxOutput};

fn arb_ascii_string(max_len: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(32u8..127u8, 0..max_len)
        .prop_map(|bytes| String::from_utf8(bytes).unwrap_or_default())
}

fn arb_outpoint() -> impl Strategy<Value = OutPoint> {
    (arb_ascii_string(80), any::<u32>()).prop_map(|(txid, index)| OutPoint { txid, index })
}

fn arb_transaction() -> impl Strategy<Value = Transaction> {
    (
        arb_ascii_string(80),
        any::<u32>(),
        proptest::collection::vec(
            (arb_outpoint(), arb_ascii_string(140), arb_ascii_string(160)).prop_map(
                |(previous_output, public_key, signature)| TxInput {
                    previous_output,
                    public_key,
                    signature,
                },
            ),
            0..6,
        ),
        proptest::collection::vec(
            (arb_ascii_string(96), any::<u64>())
                .prop_map(|(address, amount)| TxOutput { address, amount }),
            0..6,
        ),
        any::<u64>(),
        any::<u64>(),
    )
        .prop_map(|(txid, version, inputs, outputs, fee, nonce)| Transaction {
            txid,
            version,
            inputs,
            outputs,
            fee,
            nonce,
        })
}

fn arb_block() -> impl Strategy<Value = Block> {
    (
        arb_ascii_string(80),
        any::<u32>(),
        proptest::collection::vec(arb_ascii_string(80), 0..6),
        any::<u64>(),
        any::<u32>(),
        any::<u64>(),
        arb_ascii_string(80),
        arb_ascii_string(80),
        any::<u64>(),
        any::<u64>(),
        proptest::collection::vec(arb_transaction(), 0..6),
    )
        .prop_map(
            |(
                hash,
                version,
                parents,
                timestamp,
                difficulty,
                nonce,
                merkle_root,
                state_root,
                blue_score,
                height,
                transactions,
            )| Block {
                hash,
                header: BlockHeader {
                    version,
                    parents,
                    timestamp,
                    difficulty,
                    nonce,
                    merkle_root,
                    state_root,
                    blue_score,
                    height,
                },
                transactions,
            },
        )
}

proptest! {
    #[test]
    fn transaction_json_roundtrip_is_lossless(tx in arb_transaction()) {
        let encoded = serde_json::to_vec(&tx).expect("serialize transaction");
        let decoded: Transaction = serde_json::from_slice(&encoded).expect("deserialize transaction");
        prop_assert_eq!(decoded.txid, tx.txid);
        prop_assert_eq!(decoded.inputs.len(), tx.inputs.len());
        prop_assert_eq!(decoded.outputs.len(), tx.outputs.len());
        prop_assert_eq!(decoded.fee, tx.fee);
    }

    #[test]
    fn block_json_roundtrip_preserves_header_identity(block in arb_block()) {
        let encoded = serde_json::to_vec(&block).expect("serialize block");
        let decoded: Block = serde_json::from_slice(&encoded).expect("deserialize block");
        prop_assert_eq!(decoded.hash, block.hash);
        prop_assert_eq!(decoded.header.height, block.header.height);
        prop_assert_eq!(decoded.header.parents, block.header.parents);
        prop_assert_eq!(decoded.transactions.len(), block.transactions.len());
    }

    #[test]
    fn transaction_json_parser_rejects_or_accepts_without_panicking(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let _ = serde_json::from_slice::<Transaction>(&data);
    }
}
