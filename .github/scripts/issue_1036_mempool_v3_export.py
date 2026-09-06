from pathlib import Path

path = Path("crates/pulsedag-core/src/lib.rs")
text = path.read_text(encoding="utf-8")

anchors = [
    (
        "pub mod mempool;\npub mod mempool_protocol;\n",
        "pub mod mempool;\npub mod mempool_protocol;\npub mod mempool_v3;\n",
        "mempool module declarations",
    ),
    (
        "pub use mempool_protocol::reconcile_mempool_for_protocol;\n",
        "pub use mempool_protocol::reconcile_mempool_for_protocol;\n"
        "pub use mempool_v3::{\n"
        "    admission_order_key_v3, canonical_transaction_size_for_mempool_v3, fee_rate_v3,\n"
        "    FeeRateV3, MempoolPolicyAssessmentErrorV3, MempoolPolicyRejectionV3, MempoolPolicyV3,\n"
        "    FEE_RATE_SCALE_BYTES_V3, MEMPOOL_POLICY_V3_COMPAT_MAX_TRANSACTIONS,\n"
        "    MEMPOOL_POLICY_V3_COMPAT_MAX_TRANSACTION_FEE,\n"
        "    MEMPOOL_POLICY_V3_COMPAT_MIN_RELAY_FEE_RATE, MEMPOOL_POLICY_V3_VERSION,\n"
        "};\n",
        "mempool exports",
    ),
]

for old, new, label in anchors:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one {label} anchor, found {count}")
    text = text.replace(old, new, 1)

path.write_text(text, encoding="utf-8")
