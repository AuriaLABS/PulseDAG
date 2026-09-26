use crate::api::RpcStateLike;
use pulsedag_storage::Storage;

pub const MONETARY_V3_LEGACY_MINING_DISABLED: &str = "MONETARY_V3_LEGACY_MINING_DISABLED";

/// Fail closed on every retained height-subsidy mining surface once a v3
/// monetary activation sidecar is present.
///
/// Sidecar presence is intentionally sufficient to disable legacy issuance:
/// a corrupt or identity-mismatched sidecar must never fall back to the old
/// 50-coin/height-based path. The canonical monetary endpoints perform the
/// stronger identity/cadence/finality checks before issuing or accepting work.
fn ensure_storage_has_no_monetary_v3_activation(
    storage: &Storage,
    surface: &str,
) -> Result<(), String> {
    match storage.protocol_monetary_activation_record() {
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

pub fn ensure_legacy_mining_disabled_when_monetary_v3_active<S: RpcStateLike>(
    state: &S,
    surface: &str,
) -> Result<(), String> {
    ensure_storage_has_no_monetary_v3_activation(&state.storage(), surface)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        init_chain_state_v3, MonetaryCadenceSegment, ProtocolActivationIdentity,
        GHOSTDAG_V1_FINALITY_POLICY_VERSION, GHOSTDAG_V1_ORDERING_VERSION,
    };

    const ONE_SECOND: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 1_000_000_000,
    }];

    fn temp_db_path(name: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("pulsedag-rpc-monetary-guard-{name}-{nanos}"))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn legacy_mining_remains_available_without_monetary_sidecar() {
        let path = temp_db_path("legacy");
        let storage = Storage::open(&path).unwrap();
        assert!(ensure_storage_has_no_monetary_v3_activation(&storage, "/mine").is_ok());
        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn monetary_sidecar_disables_every_legacy_mining_surface() {
        let path = temp_db_path("active");
        let storage = Storage::open(&path).unwrap();
        let state = init_chain_state_v3("rpc-monetary-guard".to_string(), 1_800_000_000).unwrap();
        let identity = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        storage
            .persist_chain_state_with_monetary_protocol_record(
                &state,
                &identity,
                &ONE_SECOND,
                GHOSTDAG_V1_FINALITY_POLICY_VERSION,
            )
            .unwrap();

        for surface in [
            "/mine",
            "/mine/preview",
            "/mining/jobs/claim",
            "/mining/jobs/submit",
            "/pow/auto-run",
            "/pow/mine-capture",
            "/mining/template legacy fallback",
            "/mining/submit legacy-v1 fallback",
        ] {
            let error =
                ensure_storage_has_no_monetary_v3_activation(&storage, surface).expect_err(surface);
            assert!(error.contains("disabled while v3 monetary activation"));
            assert!(error.contains("/mining/template and /mining/submit"));
        }

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }
}
