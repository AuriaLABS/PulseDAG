use crate::api::RpcStateLike;

pub const MONETARY_V3_LEGACY_MINING_DISABLED: &str =
    "MONETARY_V3_LEGACY_MINING_DISABLED";

/// Fail closed on every retained height-subsidy mining surface once a v3
/// monetary activation sidecar is present.
///
/// Sidecar presence is intentionally sufficient to disable legacy issuance:
/// a corrupt or identity-mismatched sidecar must never fall back to the old
/// 50-coin/height-based path. The canonical monetary endpoints perform the
/// stronger identity/cadence/finality checks before issuing or accepting work.
pub fn ensure_legacy_mining_disabled_when_monetary_v3_active<S: RpcStateLike>(
    state: &S,
    surface: &str,
) -> Result<(), String> {
    match state.storage().protocol_monetary_activation_record() {
        Ok(None) => Ok(()),
        Ok(Some(record)) => Err(format!(
            "{surface} is disabled while v3 monetary activation {} is present; use /mining/template and /mining/submit",
            record.binding_fingerprint
        )),
        Err(error) => Err(format!(
            "{surface} cannot verify the v3 monetary activation sidecar and therefore fails closed: {error}"
        )),
    }
}
