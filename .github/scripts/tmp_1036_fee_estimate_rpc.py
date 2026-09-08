from pathlib import Path

p = Path('crates/pulsedag-rpc/src/handlers/tx_protocol.rs')
s = p.read_text()

anchor = '''use pulsedag_core::{
    accept_transaction_with_mempool_policy_v3,'''
assert anchor in s
s = s.replace(anchor, '''use pulsedag_core::mempool_v3::{
    estimate_mempool_fee_rates_v3, MempoolFeeEstimateV3,
};
use pulsedag_core::{
    accept_transaction_with_mempool_policy_v3,''', 1)

anchor = '''pub use super::tx_legacy::{
    get_mempool, get_tx, get_tx_lookup, get_txs, get_txs_activity, get_txs_page, get_txs_recent,
    post_tx_build, MempoolData, TxActivityData, TxActivityItem, TxDetailData, TxListData,
    TxListItem, TxLookupData, TxValidateData, TxsPageQuery, TxsQuery,
};
'''
assert anchor in s
insert = anchor + r'''
/// Public wire view of the deterministic v3 fee estimator.
///
/// Fee-rate fields are decimal strings so the RPC preserves the complete u128
/// range without depending on JSON number implementation limits.
#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct MempoolFeeEstimateData {
    pub version: u32,
    pub policy_fingerprint: String,
    pub min_relay_fee_rate_per_kb: String,
    pub economy_fee_rate_per_kb: String,
    pub standard_fee_rate_per_kb: String,
    pub priority_fee_rate_per_kb: String,
    pub observed_min_fee_rate_per_kb: Option<String>,
    pub observed_max_fee_rate_per_kb: Option<String>,
    pub mempool_transactions: u64,
    pub effective_max_transactions: u64,
    pub pressure_bps: u64,
}

impl From<MempoolFeeEstimateV3> for MempoolFeeEstimateData {
    fn from(estimate: MempoolFeeEstimateV3) -> Self {
        Self {
            version: estimate.version,
            policy_fingerprint: estimate.policy_fingerprint,
            min_relay_fee_rate_per_kb: estimate.min_relay_fee_rate_per_kb.to_string(),
            economy_fee_rate_per_kb: estimate.economy_fee_rate_per_kb.to_string(),
            standard_fee_rate_per_kb: estimate.standard_fee_rate_per_kb.to_string(),
            priority_fee_rate_per_kb: estimate.priority_fee_rate_per_kb.to_string(),
            observed_min_fee_rate_per_kb: estimate
                .observed_min_fee_rate_per_kb
                .map(|value| value.to_string()),
            observed_max_fee_rate_per_kb: estimate
                .observed_max_fee_rate_per_kb
                .map(|value| value.to_string()),
            mempool_transactions: estimate.mempool_transactions,
            effective_max_transactions: estimate.effective_max_transactions,
            pressure_bps: estimate.pressure_bps,
        }
    }
}

pub async fn get_mempool_fee_estimate<S: RpcStateLike>(
    State(state): State<S>,
) -> Json<ApiResponse<MempoolFeeEstimateData>> {
    let policy = MempoolPolicyV3::compatibility_default();
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    match estimate_mempool_fee_rates_v3(&chain, policy) {
        Ok(estimate) => Json(ApiResponse::ok(estimate.into())),
        Err(error) => Json(ApiResponse::err(
            "MEMPOOL_FEE_ESTIMATE_ERROR",
            format!("fee estimate unavailable for active mempool policy: {error:?}"),
        )),
    }
}
'''
s = s.replace(anchor, insert, 1)

test = r'''

    #[tokio::test]
    async fn mempool_fee_estimate_rpc_is_versioned_and_policy_bound() {
        let mut chain = init_chain_state("fee-estimate-rpc".into());
        chain.mempool.max_transactions = 50;
        let state = test_state(chain, None, "fee-estimate-rpc");

        let Json(response) = get_mempool_fee_estimate(State(state)).await;
        assert!(response.ok);
        assert!(response.error.is_none());
        let data = response.data.expect("fee estimate response data");
        let policy = MempoolPolicyV3::compatibility_default();
        assert_eq!(
            data.version,
            pulsedag_core::mempool_v3::MEMPOOL_FEE_ESTIMATE_V3_VERSION
        );
        assert_eq!(data.policy_fingerprint, policy.fingerprint());
        assert_eq!(data.mempool_transactions, 0);
        assert_eq!(data.effective_max_transactions, 50);
        assert_eq!(data.pressure_bps, 0);
        assert_eq!(
            data.economy_fee_rate_per_kb,
            policy.min_relay_fee_rate_per_kb.to_string()
        );
        assert_eq!(data.standard_fee_rate_per_kb, data.economy_fee_rate_per_kb);
        assert_eq!(data.priority_fee_rate_per_kb, data.economy_fee_rate_per_kb);
    }

    #[test]
    fn mempool_fee_estimate_rpc_preserves_full_u128_fee_rate_range() {
        let wire = MempoolFeeEstimateData::from(MempoolFeeEstimateV3 {
            version: pulsedag_core::mempool_v3::MEMPOOL_FEE_ESTIMATE_V3_VERSION,
            policy_fingerprint: "fee-estimate-u128-vector".into(),
            min_relay_fee_rate_per_kb: u128::MAX,
            economy_fee_rate_per_kb: u128::MAX,
            standard_fee_rate_per_kb: u128::MAX,
            priority_fee_rate_per_kb: u128::MAX,
            observed_min_fee_rate_per_kb: Some(u128::MAX),
            observed_max_fee_rate_per_kb: Some(u128::MAX),
            mempool_transactions: 1,
            effective_max_transactions: 1,
            pressure_bps: 10_000,
        });
        let json = serde_json::to_value(&wire).expect("u128-safe fee estimate JSON");
        assert_eq!(
            json["priority_fee_rate_per_kb"],
            serde_json::Value::String(u128::MAX.to_string())
        );
        assert_eq!(
            json["observed_max_fee_rate_per_kb"],
            serde_json::Value::String(u128::MAX.to_string())
        );
    }
'''
idx = s.rfind('\n}')
assert idx != -1
s = s[:idx] + test + s[idx:]
p.write_text(s)

p = Path('crates/pulsedag-rpc/src/routes.rs')
s = p.read_text()
old = '''        tx::{
            get_mempool, get_tx, get_tx_lookup, get_txs, get_txs_activity, get_txs_page,
            get_txs_recent, post_tx_build, post_tx_submit,
        },'''
new = '''        tx::{
            get_mempool, get_mempool_fee_estimate, get_tx, get_tx_lookup, get_txs,
            get_txs_activity, get_txs_page, get_txs_recent, post_tx_build, post_tx_submit,
        },'''
assert old in s
s = s.replace(old, new, 1)
old = '.route("/mempool", get(get_mempool::<S>))'
assert s.count(old) == 2
s = s.replace(old, old + '\n        .route("/mempool/fee-estimate", get(get_mempool_fee_estimate::<S>))')
p.write_text(s)
