use axum::{extract::State, Json};

use crate::{
    api::{ApiResponse, RpcStateLike},
    handlers::release::{operator_stage, repo_version},
};

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct MempoolPolicyV3Data {
    pub version: u32,
    pub fingerprint: String,
    pub min_relay_fee_rate_per_kb: String,
    pub max_transaction_fee: String,
    pub max_transactions: u64,
    pub replacement_enabled: bool,
}

impl From<pulsedag_core::MempoolPolicyV3> for MempoolPolicyV3Data {
    fn from(policy: pulsedag_core::MempoolPolicyV3) -> Self {
        Self {
            version: policy.version,
            fingerprint: policy.fingerprint(),
            min_relay_fee_rate_per_kb: policy.min_relay_fee_rate_per_kb.to_string(),
            max_transaction_fee: policy.max_transaction_fee.to_string(),
            max_transactions: policy.max_transactions,
            replacement_enabled: policy.replacement_enabled,
        }
    }
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct MempoolResourcePolicyV1Data {
    pub version: u32,
    pub fingerprint: String,
    pub max_transactions: u64,
    pub max_spent_outpoints: u64,
    pub max_orphans: u64,
    pub max_transaction_bytes: u64,
    pub live_max_age_blocks: u64,
    pub orphan_max_age_blocks: u64,
    pub expiry_boundary_version: u32,
}

impl From<pulsedag_core::MempoolResourcePolicyV1> for MempoolResourcePolicyV1Data {
    fn from(policy: pulsedag_core::MempoolResourcePolicyV1) -> Self {
        Self {
            version: policy.version,
            fingerprint: policy.fingerprint(),
            max_transactions: policy.max_transactions,
            max_spent_outpoints: policy.max_spent_outpoints,
            max_orphans: policy.max_orphans,
            max_transaction_bytes: policy.max_transaction_bytes,
            live_max_age_blocks: policy.live_max_age_blocks,
            orphan_max_age_blocks: policy.orphan_max_age_blocks,
            expiry_boundary_version: policy.expiry_boundary_version,
        }
    }
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct MonetaryPolicyV3Data {
    pub activation_state: &'static str,
    pub policy_version: &'static str,
    pub policy_fingerprint: String,
    pub production_cadence_fingerprint: Option<String>,
    pub production_cadence_frozen: bool,
    pub symbol: &'static str,
    pub decimals: u32,
    pub atoms_per_coin: String,
    pub max_supply_atoms: String,
    pub genesis_issuance_atoms: String,
    pub year1_mining_budget_atoms: String,
    pub economic_year_seconds: u64,
    pub coinbase_maturity_seconds: u64,
    pub ordinary_fee_recipient_bps: u16,
    pub consensus_burn_bps: u16,
    pub tail_emission_atoms: String,
    pub programmable_resource_fees_active: bool,
}

impl MonetaryPolicyV3Data {
    pub fn candidate_unactivated() -> Self {
        Self {
            activation_state: "candidate_not_activated",
            policy_version: pulsedag_core::MONETARY_POLICY_VERSION_V3,
            policy_fingerprint: pulsedag_core::monetary_policy_fingerprint_v3(),
            production_cadence_fingerprint: None,
            production_cadence_frozen: false,
            symbol: pulsedag_core::PDG_SYMBOL_V3,
            decimals: pulsedag_core::PDG_DECIMALS_V3,
            atoms_per_coin: pulsedag_core::ATOMS_PER_COIN.to_string(),
            max_supply_atoms: pulsedag_core::MAX_SUPPLY_ATOMS.to_string(),
            genesis_issuance_atoms: pulsedag_core::GENESIS_ISSUANCE_ATOMS.to_string(),
            year1_mining_budget_atoms: pulsedag_core::YEAR1_MINING_BUDGET_ATOMS.to_string(),
            economic_year_seconds: pulsedag_core::ECONOMIC_YEAR_SECONDS,
            coinbase_maturity_seconds: pulsedag_core::COINBASE_MATURITY_SECONDS,
            ordinary_fee_recipient_bps: pulsedag_core::ORDINARY_FEE_RECIPIENT_BPS,
            consensus_burn_bps: pulsedag_core::CONSENSUS_BURN_BPS,
            tail_emission_atoms: pulsedag_core::TAIL_EMISSION_ATOMS.to_string(),
            programmable_resource_fees_active: false,
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct PolicyData {
    pub version: String,
    pub stage: String,
    pub mempool_v3: MempoolPolicyV3Data,
    pub mempool_resource_v1: MempoolResourcePolicyV1Data,
    pub monetary_v3: MonetaryPolicyV3Data,
    pub mempool_policy: Vec<String>,
    pub transaction_rules: Vec<String>,
    pub block_rules: Vec<String>,
    pub dag_rules: Vec<String>,
    pub target_block_interval_secs: u64,
    pub window_size: usize,
    pub retarget_multiplier_bps: u64,
    pub suggested_difficulty: u64,
}

pub async fn get_policy<S: RpcStateLike>(State(state): State<S>) -> Json<ApiResponse<PolicyData>> {
    let chain_handle = state.chain();
    let chain = chain_handle.read().await;
    let snapshot = pulsedag_core::dev_difficulty_snapshot(&chain);
    let mempool_v3 = pulsedag_core::MempoolPolicyV3::production_default().into();
    let mempool_resource_v1 = pulsedag_core::MempoolResourcePolicyV1::production_default().into();

    Json(ApiResponse::ok(PolicyData {
        version: repo_version(),
        stage: operator_stage(),
        mempool_v3,
        mempool_resource_v1,
        monetary_v3: MonetaryPolicyV3Data::candidate_unactivated(),
        mempool_policy: vec![
            "reject double spends".into(),
            "require structurally valid transactions".into(),
            "keep pending transactions in in-memory mempool".into(),
            "prioritize visible fee-bearing transactions in explorer recent views".into(),
        ],
        transaction_rules: vec![
            "transaction must have a stable txid".into(),
            "inputs must reference spendable utxos".into(),
            "sum(inputs) must cover amount plus fee".into(),
            "coinbase transactions are only valid in mined blocks".into(),
        ],
        block_rules: vec![
            "block parents must be declared".into(),
            "block height must follow dag best height progression".into(),
            "coinbase reward is included by miner address".into(),
            "block acceptance mutates chain state only after validation".into(),
        ],
        dag_rules: vec![
            "genesis block must exist".into(),
            "tips set must not be empty".into(),
            "persisted and in-memory blocks should remain aligned".into(),
            "sync diagnostics should be checked before release".into(),
        ],
        target_block_interval_secs: snapshot.policy.target_block_interval_secs,
        window_size: snapshot.policy.window_size,
        retarget_multiplier_bps: snapshot.retarget_multiplier_bps,
        suggested_difficulty: snapshot.suggested_difficulty,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monetary_v3_policy_wire_view_is_explicitly_non_activating() {
        let data = MonetaryPolicyV3Data::candidate_unactivated();
        assert_eq!(data.activation_state, "candidate_not_activated");
        assert!(!data.production_cadence_frozen);
        assert_eq!(data.production_cadence_fingerprint, None);
        assert_eq!(
            data.policy_fingerprint,
            pulsedag_core::MONETARY_POLICY_FINGERPRINT_V3
        );
        assert_eq!(data.symbol, "PDG");
        assert_eq!(data.decimals, 8);
        assert_eq!(data.max_supply_atoms, "100000000000000000");
        assert!(!data.programmable_resource_fees_active);
    }

    #[test]
    fn monetary_v3_large_amounts_are_json_strings_not_unsafe_floats() {
        let json = serde_json::to_value(MonetaryPolicyV3Data::candidate_unactivated())
            .expect("serialize monetary policy v3");
        assert_eq!(
            json["max_supply_atoms"],
            serde_json::Value::String("100000000000000000".into())
        );
        assert_eq!(
            json["year1_mining_budget_atoms"],
            serde_json::Value::String("20629947401590026".into())
        );
    }

    #[test]
    fn mempool_v3_policy_wire_view_is_identity_bound_and_lossless() {
        let policy = pulsedag_core::MempoolPolicyV3::production_default();
        let data = MempoolPolicyV3Data::from(policy);

        assert_eq!(data.version, pulsedag_core::MEMPOOL_POLICY_V3_VERSION);
        assert_eq!(data.fingerprint, policy.fingerprint());
        assert_eq!(
            data.min_relay_fee_rate_per_kb,
            policy.min_relay_fee_rate_per_kb.to_string()
        );
        assert_eq!(
            data.max_transaction_fee,
            policy.max_transaction_fee.to_string()
        );
        assert_eq!(data.max_transactions, policy.max_transactions);
        assert_eq!(data.replacement_enabled, policy.replacement_enabled);
    }

    #[test]
    fn resource_policy_wire_view_is_separate_and_frozen() {
        let policy = pulsedag_core::MempoolResourcePolicyV1::production_default();
        let data = MempoolResourcePolicyV1Data::from(policy);
        assert_eq!(data.version, 1);
        assert_eq!(data.fingerprint, policy.fingerprint());
        assert_eq!(data.max_transactions, 4_096);
        assert_eq!(data.max_spent_outpoints, 8_192);
        assert_eq!(data.max_orphans, 512);
        assert_eq!(data.max_transaction_bytes, 65_536);
        assert_eq!(data.live_max_age_blocks, 1_440);
        assert_eq!(data.orphan_max_age_blocks, 1_440);
        assert_eq!(data.expiry_boundary_version, 1);
    }

    #[test]
    fn mempool_v3_policy_fee_bounds_preserve_full_u64_json_range() {
        let data = MempoolPolicyV3Data::from(pulsedag_core::MempoolPolicyV3 {
            min_relay_fee_rate_per_kb: u64::MAX,
            max_transaction_fee: u64::MAX,
            ..pulsedag_core::MempoolPolicyV3::compatibility_default()
        });
        let json = serde_json::to_value(data).expect("serialize mempool v3 policy wire view");

        assert_eq!(
            json["min_relay_fee_rate_per_kb"],
            serde_json::Value::String(u64::MAX.to_string())
        );
        assert_eq!(
            json["max_transaction_fee"],
            serde_json::Value::String(u64::MAX.to_string())
        );
    }
}
