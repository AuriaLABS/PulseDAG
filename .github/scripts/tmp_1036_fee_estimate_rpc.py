from pathlib import Path

p = Path('crates/pulsedag-rpc/src/handlers/tx_protocol.rs')
s = p.read_text()

anchor = '''use pulsedag_core::{
    accept_transaction_with_mempool_policy_v3,'''
assert anchor in s
s = s.replace(anchor, '''use pulsedag_core::mempool_v3::{
    estimate_mempool_fee_rates_v3, MempoolFeeEstimateV3, MEMPOOL_FEE_ESTIMATE_V3_VERSION,
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
#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct MempoolFeeEstimateData {
    pub version: u32,
    pub policy_fingerprint: String,
    pub min_relay_fee_rate_per_kb: u128,
    pub economy_fee_rate_per_kb: u128,
    pub standard_fee_rate_per_kb: u128,
    pub priority_fee_rate_per_kb: u128,
    pub observed_min_fee_rate_per_kb: Option<u128>,
    pub observed_max_fee_rate_per_kb: Option<u128>,
    pub mempool_transactions: u64,
    pub effective_max_transactions: u64,
    pub pressure_bps: u64,
}

impl From<MempoolFeeEstimateV3> for MempoolFeeEstimateData {
    fn from(estimate: MempoolFeeEstimateV3) -> Self {
        Self {
            version: estimate.version,
            policy_fingerprint: estimate.policy_fingerprint,
            min_relay_fee_rate_per_kb: estimate.min_relay_fee_rate_per_kb,
            economy_fee_rate_per_kb: estimate.economy_fee_rate_per_kb,
            standard_fee_rate_per_kb: estimate.standard_fee_rate_per_kb,
            priority_fee_rate_per_kb: estimate.priority_fee_rate_per_kb,
            observed_min_fee_rate_per_kb: estimate.observed_min_fee_rate_per_kb,
            observed_max_fee_rate_per_kb: estimate.observed_max_fee_rate_per_kb,
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
        assert_eq!(data.version, MEMPOOL_FEE_ESTIMATE_V3_VERSION);
        assert_eq!(data.policy_fingerprint, policy.fingerprint());
        assert_eq!(data.mempool_transactions, 0);
        assert_eq!(data.effective_max_transactions, 50);
        assert_eq!(data.pressure_bps, 0);
        assert_eq!(
            data.economy_fee_rate_per_kb,
            u128::from(policy.min_relay_fee_rate_per_kb)
        );
        assert_eq!(data.standard_fee_rate_per_kb, data.economy_fee_rate_per_kb);
        assert_eq!(data.priority_fee_rate_per_kb, data.economy_fee_rate_per_kb);
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
