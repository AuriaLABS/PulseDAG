use axum::{
    extract::{Path, Query},
    Json,
};
use serde::Deserialize;

use pulsedag_core::{
    BasedAppAdmissionV0, BasedAppCanonicalEventKindV0, BasedAppCanonicalEventV0,
    BasedAppStateViewV0, BASED_APP_DOMAIN_V0, BASED_APP_EVENT_PAGE_MAX_V0,
    BASED_APP_EVENT_RETAINED_MAX_V0, BASED_APP_EVENT_SCHEMA_VERSION_V0,
    BASED_APP_STATE_SCHEMA_VERSION_V0,
};

use crate::api::ApiResponse;

/// Wired but fail-closed until the v3 Based Apps activation contract admits the surface and
/// a bounded canonical state store is connected to RPC.
pub const BASED_APP_RPC_ADMISSION_V0: BasedAppAdmissionV0 = BasedAppAdmissionV0::INACTIVE;
pub const BASED_APP_RPC_DEFAULT_EVENT_LIMIT_V0: usize = 64;

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct BasedAppEventQueryV0 {
    pub after: Option<u64>,
    pub limit: Option<usize>,
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct BasedAppPendingStateV0Data {
    pub round: u64,
    pub prev_state_root: String,
    pub next_state_root: String,
    pub opened_pulse_height: u64,
    pub challenged: bool,
    pub challenged_pulse_height: Option<u64>,
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct BasedAppStateV0Data {
    pub schema_version: u16,
    pub domain: String,
    pub app_id: String,
    pub settled_round: Option<u64>,
    pub settled_state_root: Option<String>,
    pub pending: Option<BasedAppPendingStateV0Data>,
    pub last_canonical_position: Option<u64>,
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct BasedAppEventV0Data {
    pub schema_version: u16,
    pub canonical_position: u64,
    pub round: u64,
    pub kind: String,
    pub prev_state_root: String,
    pub next_state_root: String,
    pub pulse_height: u64,
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct BasedAppEventsV0Data {
    pub event_schema_version: u16,
    pub domain: String,
    pub app_id: String,
    pub after: Option<u64>,
    pub limit: usize,
    pub retained_max: usize,
    pub events: Vec<BasedAppEventV0Data>,
    pub next_after: Option<u64>,
}

fn based_app_rpc_error(
    code: &'static str,
    message: impl Into<String>,
) -> ApiResponse<serde_json::Value> {
    ApiResponse::err(code, message.into())
}

fn parse_app_id_v0(value: &str) -> Result<[u8; 32], &'static str> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("app_id must be exactly 32 bytes of hexadecimal");
    }
    let decoded = hex::decode(value).map_err(|_| "app_id must be hexadecimal")?;
    if decoded.len() != 32 || hex::encode(&decoded) != value {
        return Err("app_id must use canonical lowercase hexadecimal");
    }
    decoded
        .try_into()
        .map_err(|_| "app_id must decode to exactly 32 bytes")
}

pub fn based_app_event_limit_v0(query: &BasedAppEventQueryV0) -> Result<usize, &'static str> {
    let limit = query.limit.unwrap_or(BASED_APP_RPC_DEFAULT_EVENT_LIMIT_V0);
    if limit == 0 || limit > BASED_APP_EVENT_PAGE_MAX_V0 {
        return Err("limit must be within 1..=256");
    }
    Ok(limit)
}

fn event_kind_name(kind: BasedAppCanonicalEventKindV0) -> &'static str {
    match kind {
        BasedAppCanonicalEventKindV0::Opened => "opened",
        BasedAppCanonicalEventKindV0::Challenged => "challenged",
        BasedAppCanonicalEventKindV0::Settled => "settled",
        BasedAppCanonicalEventKindV0::Rejected => "rejected",
    }
}

pub fn based_app_state_data_v0(
    state: &BasedAppStateViewV0,
) -> Result<BasedAppStateV0Data, &'static str> {
    if state.schema_version != BASED_APP_STATE_SCHEMA_VERSION_V0 {
        return Err("unsupported Based Apps state schema version");
    }
    Ok(BasedAppStateV0Data {
        schema_version: state.schema_version,
        domain: BASED_APP_DOMAIN_V0.to_string(),
        app_id: hex::encode(state.app_id),
        settled_round: state.settled_round,
        settled_state_root: state.settled_state_root.map(hex::encode),
        pending: state
            .pending
            .as_ref()
            .map(|pending| BasedAppPendingStateV0Data {
                round: pending.round,
                prev_state_root: hex::encode(pending.prev_state_root),
                next_state_root: hex::encode(pending.next_state_root),
                opened_pulse_height: pending.opened_pulse_height,
                challenged: pending.challenged_pulse_height.is_some(),
                challenged_pulse_height: pending.challenged_pulse_height,
            }),
        last_canonical_position: state.last_canonical_position,
    })
}

pub fn based_app_events_data_v0(
    app_id: [u8; 32],
    after: Option<u64>,
    limit: usize,
    events: &[BasedAppCanonicalEventV0],
) -> Result<BasedAppEventsV0Data, &'static str> {
    if limit == 0 || limit > BASED_APP_EVENT_PAGE_MAX_V0 {
        return Err("limit must be within 1..=256");
    }
    if events.len() > limit || events.len() > BASED_APP_EVENT_PAGE_MAX_V0 {
        return Err("event page exceeds the requested bounded limit");
    }
    if events
        .iter()
        .any(|event| event.schema_version != BASED_APP_EVENT_SCHEMA_VERSION_V0)
    {
        return Err("unsupported Based Apps event schema version");
    }
    let rows = events
        .iter()
        .map(|event| BasedAppEventV0Data {
            schema_version: event.schema_version,
            canonical_position: event.canonical_position,
            round: event.round,
            kind: event_kind_name(event.kind).to_string(),
            prev_state_root: hex::encode(event.prev_state_root),
            next_state_root: hex::encode(event.next_state_root),
            pulse_height: event.pulse_height,
        })
        .collect::<Vec<_>>();
    let next_after = rows.last().map(|event| event.canonical_position);

    Ok(BasedAppEventsV0Data {
        event_schema_version: BASED_APP_EVENT_SCHEMA_VERSION_V0,
        domain: BASED_APP_DOMAIN_V0.to_string(),
        app_id: hex::encode(app_id),
        after,
        limit,
        retained_max: BASED_APP_EVENT_RETAINED_MAX_V0,
        events: rows,
        next_after,
    })
}

/// Canonical Based Apps state surface. It is deliberately wired before activation so route
/// identity and public exposure can be tested, but the current v3 candidate fails closed.
pub async fn get_based_app_state(
    Path(app_id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    if !BASED_APP_RPC_ADMISSION_V0.admitted() {
        return Json(based_app_rpc_error(
            "based_app_surface_disabled",
            "Based Apps v0 public state surface is not admitted on this candidate",
        ));
    }

    if let Err(message) = parse_app_id_v0(&app_id) {
        return Json(based_app_rpc_error("based_app_invalid_app_id", message));
    }

    Json(based_app_rpc_error(
        "based_app_state_store_unavailable",
        "bounded canonical Based Apps state storage is not connected",
    ))
}

/// Cursor-based canonical event surface. No unbounded history query is accepted.
pub async fn get_based_app_events(
    Path(app_id): Path<String>,
    Query(query): Query<BasedAppEventQueryV0>,
) -> Json<ApiResponse<serde_json::Value>> {
    if !BASED_APP_RPC_ADMISSION_V0.admitted() {
        return Json(based_app_rpc_error(
            "based_app_surface_disabled",
            "Based Apps v0 public event surface is not admitted on this candidate",
        ));
    }

    if let Err(message) = parse_app_id_v0(&app_id) {
        return Json(based_app_rpc_error("based_app_invalid_app_id", message));
    }
    if let Err(message) = based_app_event_limit_v0(&query) {
        return Json(based_app_rpc_error(
            "based_app_invalid_event_limit",
            message,
        ));
    }

    Json(based_app_rpc_error(
        "based_app_event_store_unavailable",
        "bounded canonical Based Apps event storage is not connected",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::BasedAppPendingRoundV0;

    #[test]
    fn current_candidate_keeps_based_app_rpc_inactive() {
        assert!(!BASED_APP_RPC_ADMISSION_V0.admitted());
    }

    #[test]
    fn app_id_requires_canonical_lowercase_hex() {
        assert_eq!(parse_app_id_v0(&"ab".repeat(32)).unwrap(), [0xab; 32]);
        assert!(parse_app_id_v0(&"AB".repeat(32)).is_err());
        assert!(parse_app_id_v0("00").is_err());
        assert!(parse_app_id_v0(&"gg".repeat(32)).is_err());
    }

    #[test]
    fn event_limit_is_bounded() {
        assert_eq!(
            based_app_event_limit_v0(&BasedAppEventQueryV0 {
                after: None,
                limit: None,
            }),
            Ok(64)
        );
        assert_eq!(
            based_app_event_limit_v0(&BasedAppEventQueryV0 {
                after: Some(10),
                limit: Some(256),
            }),
            Ok(256)
        );
        assert!(based_app_event_limit_v0(&BasedAppEventQueryV0 {
            after: None,
            limit: Some(0),
        })
        .is_err());
        assert!(based_app_event_limit_v0(&BasedAppEventQueryV0 {
            after: None,
            limit: Some(257),
        })
        .is_err());
    }

    #[test]
    fn state_and_event_json_models_are_canonical() {
        let app_id = [0x11; 32];
        let state = BasedAppStateViewV0 {
            schema_version: BASED_APP_STATE_SCHEMA_VERSION_V0,
            app_id,
            settled_round: Some(3),
            settled_state_root: Some([0x22; 32]),
            pending: Some(BasedAppPendingRoundV0 {
                round: 4,
                prev_state_root: [0x22; 32],
                next_state_root: [0x33; 32],
                opened_pulse_height: 100,
                challenged_pulse_height: Some(120),
            }),
            last_canonical_position: Some(9),
        };
        let data = based_app_state_data_v0(&state).unwrap();
        assert_eq!(data.app_id, "11".repeat(32));
        assert_eq!(data.settled_state_root, Some("22".repeat(32)));
        assert_eq!(
            data.pending
                .as_ref()
                .map(|pending| pending.next_state_root.clone()),
            Some("33".repeat(32))
        );

        let event = BasedAppCanonicalEventV0 {
            schema_version: BASED_APP_EVENT_SCHEMA_VERSION_V0,
            canonical_position: 10,
            app_id,
            round: 4,
            kind: BasedAppCanonicalEventKindV0::Challenged,
            prev_state_root: [0x22; 32],
            next_state_root: [0x33; 32],
            pulse_height: 120,
        };
        let page = based_app_events_data_v0(app_id, Some(9), 64, &[event.clone()]).unwrap();
        assert_eq!(page.next_after, Some(10));
        assert_eq!(page.events[0].kind, "challenged");
        assert_eq!(page.retained_max, BASED_APP_EVENT_RETAINED_MAX_V0);
        assert!(based_app_events_data_v0(
            app_id,
            None,
            1,
            &[
                BasedAppCanonicalEventV0 {
                    schema_version: BASED_APP_EVENT_SCHEMA_VERSION_V0,
                    canonical_position: 1,
                    app_id,
                    round: 1,
                    kind: BasedAppCanonicalEventKindV0::Opened,
                    prev_state_root: [0; 32],
                    next_state_root: [1; 32],
                    pulse_height: 1,
                },
                BasedAppCanonicalEventV0 {
                    schema_version: BASED_APP_EVENT_SCHEMA_VERSION_V0,
                    canonical_position: 2,
                    app_id,
                    round: 1,
                    kind: BasedAppCanonicalEventKindV0::Settled,
                    prev_state_root: [0; 32],
                    next_state_root: [1; 32],
                    pulse_height: 65,
                },
            ]
        )
        .is_err());

        let mut unknown_state = state.clone();
        unknown_state.schema_version = 99;
        assert!(based_app_state_data_v0(&unknown_state).is_err());

        let mut unknown_event = event;
        unknown_event.schema_version = 99;
        assert!(based_app_events_data_v0(app_id, None, 64, &[unknown_event]).is_err());
    }
}
