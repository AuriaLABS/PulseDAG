use std::{
    collections::HashMap,
    env,
    error::Error,
    fmt,
    io::{self, Write},
    net::IpAddr,
    path::PathBuf,
    time::Duration,
};

use pulsedag_wallet::{
    WalletNetworkIdentity, WalletPendingJournal, WalletPendingJournalStore, WalletPendingState,
};
use reqwest::{redirect::Policy, Client, Response, Url};
use serde::{Deserialize, Serialize};

const RESPONSE_MAX_BYTES: usize = 1024 * 1024;
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const EXPLORER_CAPABILITY: &str = "explorer_api";
const ACTIVITY_ENDPOINT: &str = "/address/:address/activity";
const DEFAULT_PAGE_SIZE: usize = 100;
const MAX_PAGE_SIZE: usize = 100;
const DEFAULT_MAX_PAGES: usize = 4;
const MAX_PAGES: usize = 10;

type CliResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug)]
struct ReconcileArgs {
    pending_journal: PathBuf,
    txid: String,
    relay: String,
    page_size: usize,
    max_pages: usize,
}

#[derive(Debug)]
struct ReconcileClientError(String);

impl fmt::Display for ReconcileClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for ReconcileClientError {}

fn reconcile_error(message: impl Into<String>) -> ReconcileClientError {
    ReconcileClientError(message.into())
}

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    code: String,
    message: String,
}

#[derive(Debug, Deserialize)]
struct ReleaseIdentityData {
    network_profile: String,
    chain_id: String,
    capabilities: Vec<String>,
    core_endpoints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct AddressActivityItem {
    txid: String,
    direction: String,
    incoming: u64,
    outgoing: u64,
    net: i64,
    context: String,
    is_mempool: bool,
    is_confirmed: bool,
    block_hash: Option<String>,
    block_height: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct AddressActivityData {
    address: String,
    count: usize,
    total: usize,
    limit: usize,
    offset: usize,
    has_more: bool,
    activity: Vec<AddressActivityItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReconcileObservation {
    Mempool,
    Confirmed,
    NotObserved,
}

#[derive(Debug)]
struct ReconcileEvidence {
    observation: ReconcileObservation,
    pages_scanned: usize,
    items_scanned: usize,
    retained_history_exhausted: bool,
    budget_exhausted: bool,
}

#[derive(Debug, Serialize)]
struct ReconcileOutput {
    network_profile: String,
    chain_id: String,
    txid: String,
    from: String,
    prior_state: String,
    state: String,
    evidence: &'static str,
    pages_scanned: usize,
    items_scanned: usize,
    retained_history_exhausted: bool,
    budget_exhausted: bool,
    journal_updated: bool,
    reservation_retained: bool,
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn required(flags: &HashMap<String, String>, name: &str) -> CliResult<String> {
    flags
        .get(name)
        .cloned()
        .ok_or_else(|| invalid_input(format!("missing required option --{name}")))
        .map_err(Into::into)
}

fn parse_usize(name: &str, value: &str) -> CliResult<usize> {
    value
        .parse::<usize>()
        .map_err(|_| invalid_input(format!("{name} must be a non-negative integer")).into())
}

fn parse_flags(args: impl Iterator<Item = String>) -> CliResult<HashMap<String, String>> {
    let mut flags = HashMap::new();
    let mut args = args.peekable();
    while let Some(flag) = args.next() {
        if !flag.starts_with("--") || flag.len() <= 2 {
            return Err(invalid_input(format!("unexpected argument: {flag}")).into());
        }
        let key = flag.trim_start_matches("--").to_string();
        let value = args
            .next()
            .ok_or_else(|| invalid_input(format!("missing value for --{key}")))?;
        if value.starts_with("--") {
            return Err(invalid_input(format!("missing value for --{key}")).into());
        }
        if flags.insert(key.clone(), value).is_some() {
            return Err(invalid_input(format!("duplicate option --{key}")).into());
        }
    }
    Ok(flags)
}

fn parse_args_from(args: impl Iterator<Item = String>) -> CliResult<ReconcileArgs> {
    let flags = parse_flags(args)?;
    for key in flags.keys() {
        if !["pending-journal", "txid", "relay", "page-size", "max-pages"]
            .iter()
            .any(|allowed| key == allowed)
        {
            return Err(invalid_input(format!("unknown option --{key}")).into());
        }
    }
    let page_size = flags
        .get("page-size")
        .map(|value| parse_usize("--page-size", value))
        .transpose()?
        .unwrap_or(DEFAULT_PAGE_SIZE);
    if page_size == 0 || page_size > MAX_PAGE_SIZE {
        return Err(
            invalid_input(format!("--page-size must be between 1 and {MAX_PAGE_SIZE}")).into(),
        );
    }
    let max_pages = flags
        .get("max-pages")
        .map(|value| parse_usize("--max-pages", value))
        .transpose()?
        .unwrap_or(DEFAULT_MAX_PAGES);
    if max_pages == 0 || max_pages > MAX_PAGES {
        return Err(invalid_input(format!("--max-pages must be between 1 and {MAX_PAGES}")).into());
    }
    Ok(ReconcileArgs {
        pending_journal: PathBuf::from(required(&flags, "pending-journal")?),
        txid: required(&flags, "txid")?,
        relay: required(&flags, "relay")?,
        page_size,
        max_pages,
    })
}

fn parse_args() -> CliResult<ReconcileArgs> {
    parse_args_from(env::args().skip(1))
}

fn validate_txid(txid: &str) -> CliResult<()> {
    let decoded = hex::decode(txid).map_err(|_| invalid_input("--txid must be hexadecimal"))?;
    if decoded.len() != 32 || hex::encode(decoded) != txid {
        return Err(invalid_input("--txid must be canonical lowercase 32-byte hexadecimal").into());
    }
    Ok(())
}

fn relay_base_url(raw: &str) -> Result<Url, ReconcileClientError> {
    let mut url = Url::parse(raw).map_err(|_| reconcile_error("relay URL is invalid"))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(reconcile_error("relay URL must not contain credentials"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(reconcile_error(
            "relay URL must not contain query or fragment",
        ));
    }
    if !matches!(url.path(), "" | "/") {
        return Err(reconcile_error(
            "relay URL must be an origin without a path",
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| reconcile_error("relay URL must contain a host"))?;
    match url.scheme() {
        "https" => {}
        "http" if is_loopback_host(host) => {}
        "http" => {
            return Err(reconcile_error(
                "plain HTTP relay URLs are allowed only for loopback development",
            ));
        }
        _ => {
            return Err(reconcile_error(
                "relay URL scheme must be https or loopback http",
            ));
        }
    }
    url.set_path("/");
    Ok(url)
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<IpAddr>()
        .map(|address| address.is_loopback())
        .unwrap_or(false)
}

fn build_client() -> Result<Client, ReconcileClientError> {
    Client::builder()
        .timeout(HTTP_TIMEOUT)
        .redirect(Policy::none())
        .build()
        .map_err(|error| {
            reconcile_error(format!("failed to initialize relay HTTP client: {error}"))
        })
}

async fn bounded_body(
    mut response: Response,
) -> Result<(reqwest::StatusCode, Vec<u8>), ReconcileClientError> {
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > RESPONSE_MAX_BYTES as u64)
    {
        return Err(reconcile_error("relay response exceeds body limit"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| reconcile_error(format!("failed reading relay response: {error}")))?
    {
        if bytes.len().saturating_add(chunk.len()) > RESPONSE_MAX_BYTES {
            return Err(reconcile_error("relay response exceeds body limit"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((status, bytes))
}

fn api_error_detail(error: Option<ApiError>) -> String {
    error
        .map(|value| format!("{}: {}", value.code, value.message))
        .unwrap_or_else(|| "missing API error detail".to_string())
}

fn validate_release_identity(
    expected: &WalletNetworkIdentity,
    response: ApiResponse<ReleaseIdentityData>,
) -> Result<(), ReconcileClientError> {
    if !response.ok {
        return Err(reconcile_error(format!(
            "relay identity request failed: {}",
            api_error_detail(response.error)
        )));
    }
    let data = response
        .data
        .ok_or_else(|| reconcile_error("relay identity response is missing data"))?;
    let observed = WalletNetworkIdentity::new(data.network_profile, data.chain_id)
        .map_err(|error| reconcile_error(format!("relay identity is invalid: {error}")))?;
    expected
        .ensure_matches(&observed)
        .map_err(|error| reconcile_error(format!("relay network mismatch: {error}")))?;
    if !data
        .capabilities
        .iter()
        .any(|capability| capability == EXPLORER_CAPABILITY)
    {
        return Err(reconcile_error(
            "relay identity is missing required capability: explorer_api",
        ));
    }
    if !data
        .core_endpoints
        .iter()
        .any(|endpoint| endpoint == ACTIVITY_ENDPOINT)
    {
        return Err(reconcile_error(format!(
            "relay identity does not advertise canonical endpoint: {ACTIVITY_ENDPOINT}"
        )));
    }
    Ok(())
}

async fn fetch_release_identity(
    client: &Client,
    base: &Url,
    expected: &WalletNetworkIdentity,
) -> Result<(), ReconcileClientError> {
    let url = base
        .join("release")
        .map_err(|_| reconcile_error("failed to construct relay identity URL"))?;
    let response =
        client.get(url).send().await.map_err(|error| {
            reconcile_error(format!("relay identity transport failed: {error}"))
        })?;
    let (status, body) = bounded_body(response).await?;
    if !status.is_success() {
        return Err(reconcile_error(format!(
            "relay identity request returned HTTP {}",
            status.as_u16()
        )));
    }
    let parsed = serde_json::from_slice::<ApiResponse<ReleaseIdentityData>>(&body)
        .map_err(|_| reconcile_error("relay identity response JSON is invalid"))?;
    validate_release_identity(expected, parsed)
}

fn activity_url(
    base: &Url,
    address: &str,
    limit: usize,
    offset: usize,
) -> Result<Url, ReconcileClientError> {
    if address.is_empty() || address.trim() != address {
        return Err(reconcile_error("wallet address is invalid"));
    }
    let mut url = base.clone();
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| reconcile_error("relay URL is not a valid base URL"))?;
        segments.pop_if_empty();
        segments.push("address");
        segments.push(address);
        segments.push("activity");
    }
    url.query_pairs_mut()
        .append_pair("limit", &limit.to_string())
        .append_pair("offset", &offset.to_string());
    Ok(url)
}

fn expected_direction(net: i64) -> &'static str {
    if net > 0 {
        "incoming"
    } else if net < 0 {
        "outgoing"
    } else {
        "self"
    }
}

fn validate_activity_item(item: &AddressActivityItem) -> Result<(), ReconcileClientError> {
    let decoded = hex::decode(&item.txid)
        .map_err(|_| reconcile_error("activity item txid is not hexadecimal"))?;
    if decoded.len() != 32 || hex::encode(decoded) != item.txid {
        return Err(reconcile_error(
            "activity item txid must be canonical lowercase 32-byte hexadecimal",
        ));
    }
    if item.direction != expected_direction(item.net) {
        return Err(reconcile_error("activity item direction/net mismatch"));
    }
    let expected_net = i128::from(item.incoming) - i128::from(item.outgoing);
    if expected_net != i128::from(item.net) {
        return Err(reconcile_error("activity item amount/net mismatch"));
    }
    match item.context.as_str() {
        "mempool" => {
            if !item.is_mempool
                || item.is_confirmed
                || item.block_hash.is_some()
                || item.block_height.is_some()
            {
                return Err(reconcile_error("mempool activity state is incoherent"));
            }
        }
        "confirmed" => {
            if item.is_mempool
                || !item.is_confirmed
                || item.block_hash.as_deref().is_none_or(str::is_empty)
                || item.block_height.is_none()
            {
                return Err(reconcile_error("confirmed activity state is incoherent"));
            }
        }
        _ => return Err(reconcile_error("activity item context is unsupported")),
    }
    Ok(())
}

fn validate_activity_response(
    address: &str,
    requested_limit: usize,
    requested_offset: usize,
    data: AddressActivityData,
) -> Result<AddressActivityData, ReconcileClientError> {
    if data.address != address {
        return Err(reconcile_error(
            "address activity response address mismatch",
        ));
    }
    if data.limit != requested_limit || data.offset != requested_offset {
        return Err(reconcile_error("address activity pagination mismatch"));
    }
    if data.count != data.activity.len() || data.count > data.limit {
        return Err(reconcile_error("address activity response count mismatch"));
    }
    if data.count == 0 {
        if data.offset < data.total {
            return Err(reconcile_error(
                "address activity empty-page total/offset mismatch",
            ));
        }
    } else if data.offset >= data.total || data.offset.saturating_add(data.count) > data.total {
        return Err(reconcile_error("address activity total/offset mismatch"));
    }
    let expected_has_more = data.offset.saturating_add(data.count) < data.total;
    if data.has_more != expected_has_more {
        return Err(reconcile_error("address activity has_more mismatch"));
    }
    for item in &data.activity {
        validate_activity_item(item)?;
    }
    Ok(data)
}

async fn fetch_activity_page(
    client: &Client,
    base: &Url,
    address: &str,
    limit: usize,
    offset: usize,
) -> Result<AddressActivityData, ReconcileClientError> {
    let response = client
        .get(activity_url(base, address, limit, offset)?)
        .send()
        .await
        .map_err(|error| reconcile_error(format!("address activity transport failed: {error}")))?;
    let (status, body) = bounded_body(response).await?;
    if !status.is_success() {
        return Err(reconcile_error(format!(
            "address activity request returned HTTP {}",
            status.as_u16()
        )));
    }
    let parsed = serde_json::from_slice::<ApiResponse<AddressActivityData>>(&body)
        .map_err(|_| reconcile_error("address activity response JSON is invalid"))?;
    if !parsed.ok {
        return Err(reconcile_error(format!(
            "address activity request failed: {}",
            api_error_detail(parsed.error)
        )));
    }
    validate_activity_response(
        address,
        limit,
        offset,
        parsed
            .data
            .ok_or_else(|| reconcile_error("address activity response is missing data"))?,
    )
}

async fn fetch_evidence(
    client: &Client,
    base: &Url,
    address: &str,
    txid: &str,
    page_size: usize,
    max_pages: usize,
) -> Result<ReconcileEvidence, ReconcileClientError> {
    let mut offset = 0usize;
    let mut items_scanned = 0usize;
    for page_index in 0..max_pages {
        let page = fetch_activity_page(client, base, address, page_size, offset).await?;
        let pages_scanned = page_index + 1;
        for item in &page.activity {
            items_scanned = items_scanned.saturating_add(1);
            if item.txid == txid {
                let observation = if item.is_confirmed {
                    ReconcileObservation::Confirmed
                } else if item.is_mempool {
                    ReconcileObservation::Mempool
                } else {
                    return Err(reconcile_error(
                        "matching activity item is neither mempool nor confirmed",
                    ));
                };
                return Ok(ReconcileEvidence {
                    observation,
                    pages_scanned,
                    items_scanned,
                    retained_history_exhausted: !page.has_more,
                    budget_exhausted: false,
                });
            }
        }
        if !page.has_more {
            return Ok(ReconcileEvidence {
                observation: ReconcileObservation::NotObserved,
                pages_scanned,
                items_scanned,
                retained_history_exhausted: true,
                budget_exhausted: false,
            });
        }
        let next_offset = page.offset.saturating_add(page.count);
        if next_offset <= offset {
            return Err(reconcile_error(
                "address activity pagination made no forward progress",
            ));
        }
        offset = next_offset;
    }
    Ok(ReconcileEvidence {
        observation: ReconcileObservation::NotObserved,
        pages_scanned: max_pages,
        items_scanned,
        retained_history_exhausted: false,
        budget_exhausted: true,
    })
}

fn apply_observation(
    journal: &mut WalletPendingJournal,
    txid: &str,
    observation: ReconcileObservation,
) -> CliResult<bool> {
    let current = journal
        .entry(txid)
        .ok_or_else(|| invalid_input("pending transaction is unknown"))?
        .state;
    match observation {
        ReconcileObservation::NotObserved => Ok(false),
        ReconcileObservation::Confirmed => {
            if current == WalletPendingState::Confirmed {
                return Ok(false);
            }
            if current == WalletPendingState::Signed {
                journal.mark_submission_started(txid)?;
            }
            journal.mark_confirmed(txid)?;
            Ok(true)
        }
        ReconcileObservation::Mempool => {
            if matches!(
                current,
                WalletPendingState::ObservedMempool | WalletPendingState::Confirmed
            ) {
                return Ok(false);
            }
            if current == WalletPendingState::Signed {
                journal.mark_submission_started(txid)?;
            }
            journal.mark_observed_mempool(txid)?;
            Ok(true)
        }
    }
}

async fn run(args: ReconcileArgs) -> CliResult<ReconcileOutput> {
    validate_txid(&args.txid)?;

    let (network, from) = {
        let store = WalletPendingJournalStore::try_acquire(&args.pending_journal)?;
        let snapshot = store
            .load_latest()?
            .ok_or_else(|| invalid_input("pending journal has no committed transactions"))?;
        snapshot.journal.validate()?;
        let entry = snapshot
            .journal
            .entry(&args.txid)
            .ok_or_else(|| invalid_input("pending transaction is unknown"))?;
        (snapshot.journal.network.clone(), entry.from.clone())
    };

    let base = relay_base_url(&args.relay)?;
    let client = build_client()?;
    fetch_release_identity(&client, &base, &network).await?;
    let evidence = fetch_evidence(
        &client,
        &base,
        &from,
        &args.txid,
        args.page_size,
        args.max_pages,
    )
    .await?;

    let store = WalletPendingJournalStore::try_acquire(&args.pending_journal)?;
    let mut snapshot = store.load_or_new(&network)?;
    let prior_state = snapshot
        .journal
        .entry(&args.txid)
        .ok_or_else(|| invalid_input("pending transaction disappeared during reconciliation"))?
        .state;
    let journal_updated =
        apply_observation(&mut snapshot.journal, &args.txid, evidence.observation)?;
    if journal_updated {
        store.save_next(snapshot.generation, &snapshot.journal)?;
    }
    let state = snapshot
        .journal
        .entry(&args.txid)
        .ok_or_else(|| invalid_input("pending transaction disappeared during reconciliation"))?
        .state;
    let evidence_name = match evidence.observation {
        ReconcileObservation::Mempool => "mempool",
        ReconcileObservation::Confirmed => "confirmed",
        ReconcileObservation::NotObserved => "not_observed",
    };

    Ok(ReconcileOutput {
        network_profile: network.network_profile,
        chain_id: network.chain_id,
        txid: args.txid,
        from,
        prior_state: prior_state.as_str().to_string(),
        state: state.as_str().to_string(),
        evidence: evidence_name,
        pages_scanned: evidence.pages_scanned,
        items_scanned: evidence.items_scanned,
        retained_history_exhausted: evidence.retained_history_exhausted,
        budget_exhausted: evidence.budget_exhausted,
        journal_updated,
        reservation_retained: state.reserves_outpoints(),
    })
}

fn write_json<T: Serialize>(value: &T) -> CliResult<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    serde_json::to_writer(&mut out, value)?;
    out.write_all(b"\n")?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let result = async {
        let args = parse_args()?;
        write_json(&run(args).await?)
    }
    .await;
    if let Err(error) = result {
        eprintln!("pulsedag-wallet-reconcile: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use pulsedag_core::types::OutPoint;
    use pulsedag_wallet::SelectedUtxo;

    use super::*;

    fn args(values: &[&str]) -> impl Iterator<Item = String> {
        values
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn selected() -> [SelectedUtxo; 1] {
        [SelectedUtxo {
            outpoint: OutPoint {
                txid: "11".repeat(32),
                index: 0,
            },
            amount: 100,
        }]
    }

    fn from_address() -> String {
        pulsedag_core::address_from_public_key(&"ab".repeat(32))
    }

    #[test]
    fn parser_is_bounded_and_rejects_secret_options() {
        let txid = "aa".repeat(32);
        let parsed = parse_args_from(args(&[
            "--pending-journal",
            "pending",
            "--txid",
            &txid,
            "--relay",
            "https://relay.example",
            "--page-size",
            "50",
            "--max-pages",
            "3",
        ]))
        .unwrap();
        assert_eq!(parsed.page_size, 50);
        assert_eq!(parsed.max_pages, 3);

        assert!(parse_args_from(args(&[
            "--pending-journal",
            "pending",
            "--txid",
            &txid,
            "--relay",
            "https://relay.example",
            "--password",
            "secret",
        ]))
        .is_err());
        assert!(parse_args_from(args(&[
            "--pending-journal",
            "pending",
            "--txid",
            &txid,
            "--relay",
            "https://relay.example",
            "--page-size",
            "101",
        ]))
        .is_err());
        assert!(parse_args_from(args(&[
            "--pending-journal",
            "pending",
            "--txid",
            &txid,
            "--relay",
            "https://relay.example",
            "--max-pages",
            "11",
        ]))
        .is_err());
    }

    #[test]
    fn identity_requires_exact_network_capability_and_activity_endpoint() {
        let network = WalletNetworkIdentity::new("testnet", "pulsedag-testnet").unwrap();
        let valid = ApiResponse {
            ok: true,
            data: Some(ReleaseIdentityData {
                network_profile: "testnet".to_string(),
                chain_id: "pulsedag-testnet".to_string(),
                capabilities: vec![EXPLORER_CAPABILITY.to_string()],
                core_endpoints: vec![ACTIVITY_ENDPOINT.to_string()],
            }),
            error: None,
        };
        assert!(validate_release_identity(&network, valid).is_ok());

        let wrong_network = ApiResponse {
            ok: true,
            data: Some(ReleaseIdentityData {
                network_profile: "testnet".to_string(),
                chain_id: "other-chain".to_string(),
                capabilities: vec![EXPLORER_CAPABILITY.to_string()],
                core_endpoints: vec![ACTIVITY_ENDPOINT.to_string()],
            }),
            error: None,
        };
        assert!(validate_release_identity(&network, wrong_network).is_err());

        let wrong_endpoint = ApiResponse {
            ok: true,
            data: Some(ReleaseIdentityData {
                network_profile: "testnet".to_string(),
                chain_id: "pulsedag-testnet".to_string(),
                capabilities: vec![EXPLORER_CAPABILITY.to_string()],
                core_endpoints: vec!["/address/:address".to_string()],
            }),
            error: None,
        };
        assert!(validate_release_identity(&network, wrong_endpoint).is_err());
    }

    #[test]
    fn activity_validation_fails_closed_on_state_txid_or_pagination() {
        let valid = AddressActivityData {
            address: "pulse1sender".to_string(),
            count: 1,
            total: 2,
            limit: 1,
            offset: 0,
            has_more: true,
            activity: vec![AddressActivityItem {
                txid: "11".repeat(32),
                direction: "outgoing".to_string(),
                incoming: 0,
                outgoing: 5,
                net: -5,
                context: "mempool".to_string(),
                is_mempool: true,
                is_confirmed: false,
                block_hash: None,
                block_height: None,
            }],
        };
        assert!(validate_activity_response("pulse1sender", 1, 0, valid).is_ok());

        let bad_txid = AddressActivityData {
            address: "pulse1sender".to_string(),
            count: 1,
            total: 1,
            limit: 1,
            offset: 0,
            has_more: false,
            activity: vec![AddressActivityItem {
                txid: "AA".repeat(32),
                direction: "outgoing".to_string(),
                incoming: 0,
                outgoing: 5,
                net: -5,
                context: "mempool".to_string(),
                is_mempool: true,
                is_confirmed: false,
                block_hash: None,
                block_height: None,
            }],
        };
        assert!(validate_activity_response("pulse1sender", 1, 0, bad_txid).is_err());

        let bad_page = AddressActivityData {
            address: "pulse1sender".to_string(),
            count: 0,
            total: 2,
            limit: 1,
            offset: 1,
            has_more: true,
            activity: Vec::new(),
        };
        assert!(validate_activity_response("pulse1sender", 1, 1, bad_page).is_err());
    }

    #[test]
    fn positive_evidence_is_monotonic_and_absence_never_releases() {
        let network = WalletNetworkIdentity::new("testnet", "pulsedag-testnet").unwrap();
        let txid = "aa".repeat(32);
        let mut journal = WalletPendingJournal::new(network).unwrap();
        journal
            .reserve_signed(&txid, from_address(), &selected())
            .unwrap();

        assert!(
            !apply_observation(&mut journal, &txid, ReconcileObservation::NotObserved).unwrap()
        );
        assert_eq!(
            journal.entry(&txid).unwrap().state,
            WalletPendingState::Signed
        );
        assert!(journal.entry(&txid).unwrap().state.reserves_outpoints());

        assert!(apply_observation(&mut journal, &txid, ReconcileObservation::Mempool).unwrap());
        assert_eq!(
            journal.entry(&txid).unwrap().state,
            WalletPendingState::ObservedMempool
        );
        assert!(journal.entry(&txid).unwrap().state.reserves_outpoints());

        assert!(apply_observation(&mut journal, &txid, ReconcileObservation::Confirmed).unwrap());
        assert_eq!(
            journal.entry(&txid).unwrap().state,
            WalletPendingState::Confirmed
        );
        assert!(!journal.entry(&txid).unwrap().state.reserves_outpoints());

        assert!(!apply_observation(&mut journal, &txid, ReconcileObservation::Mempool).unwrap());
        assert_eq!(
            journal.entry(&txid).unwrap().state,
            WalletPendingState::Confirmed
        );
    }

    #[test]
    fn rejected_and_unknown_states_accept_only_stronger_positive_evidence() {
        let network = WalletNetworkIdentity::new("testnet", "pulsedag-testnet").unwrap();

        let rejected_txid = "aa".repeat(32);
        let mut rejected = WalletPendingJournal::new(network.clone()).unwrap();
        rejected
            .reserve_signed(&rejected_txid, from_address(), &selected())
            .unwrap();
        rejected.mark_submission_started(&rejected_txid).unwrap();
        rejected
            .mark_relay_rejected(&rejected_txid, "TX_REJECTED", "generic")
            .unwrap();
        assert!(
            apply_observation(&mut rejected, &rejected_txid, ReconcileObservation::Mempool)
                .unwrap()
        );
        assert_eq!(
            rejected.entry(&rejected_txid).unwrap().state,
            WalletPendingState::ObservedMempool
        );

        let unknown_txid = "bb".repeat(32);
        let mut unknown = WalletPendingJournal::new(network).unwrap();
        unknown
            .reserve_signed(&unknown_txid, from_address(), &selected())
            .unwrap();
        unknown.mark_submission_started(&unknown_txid).unwrap();
        unknown
            .mark_submission_outcome_unknown(&unknown_txid)
            .unwrap();
        assert!(
            apply_observation(&mut unknown, &unknown_txid, ReconcileObservation::Confirmed)
                .unwrap()
        );
        assert_eq!(
            unknown.entry(&unknown_txid).unwrap().state,
            WalletPendingState::Confirmed
        );
    }

    fn durable_test_dir(label: &str) -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "pulsedag-wallet-reconcile-{label}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn durable_not_observed_survives_restart_and_retains_reservation() {
        let path = durable_test_dir("not-observed");
        let network = WalletNetworkIdentity::new("testnet", "pulsedag-testnet").unwrap();
        let txid = "cc".repeat(32);

        {
            let store = WalletPendingJournalStore::try_acquire(&path).expect("store");
            let mut snapshot = store.load_or_new(&network).expect("load");
            snapshot
                .journal
                .reserve_signed(&txid, from_address(), &selected())
                .expect("reserve");
            snapshot
                .journal
                .mark_submission_started(&txid)
                .expect("started");
            snapshot
                .journal
                .mark_submission_outcome_unknown(&txid)
                .expect("unknown");
            store
                .save_next(snapshot.generation, &snapshot.journal)
                .expect("save unknown");
        }

        {
            let store = WalletPendingJournalStore::try_acquire(&path).expect("reopen");
            let mut snapshot = store.load_or_new(&network).expect("reload");
            assert!(!apply_observation(
                &mut snapshot.journal,
                &txid,
                ReconcileObservation::NotObserved,
            )
            .expect("absence is a no-op"));
            let entry = snapshot.journal.entry(&txid).expect("entry");
            assert_eq!(entry.state, WalletPendingState::SubmissionOutcomeUnknown);
            assert!(entry.state.reserves_outpoints());
            assert_eq!(snapshot.generation, 1);
        }

        {
            let store = WalletPendingJournalStore::try_acquire(&path).expect("second reopen");
            let snapshot = store.load_or_new(&network).expect("second reload");
            let entry = snapshot.journal.entry(&txid).expect("entry");
            assert_eq!(entry.state, WalletPendingState::SubmissionOutcomeUnknown);
            assert!(entry.state.reserves_outpoints());
            assert_eq!(snapshot.generation, 1);
        }

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn durable_confirmation_releases_after_restart_and_cross_network_fails_closed() {
        let path = durable_test_dir("confirmed");
        let network = WalletNetworkIdentity::new("testnet", "pulsedag-testnet").unwrap();
        let wrong_network = WalletNetworkIdentity::new("testnet", "other-chain").unwrap();
        let txid = "dd".repeat(32);

        {
            let store = WalletPendingJournalStore::try_acquire(&path).expect("store");
            let mut snapshot = store.load_or_new(&network).expect("load");
            snapshot
                .journal
                .reserve_signed(&txid, from_address(), &selected())
                .expect("reserve");
            store
                .save_next(snapshot.generation, &snapshot.journal)
                .expect("save signed");
        }

        {
            let store = WalletPendingJournalStore::try_acquire(&path).expect("reopen");
            assert!(store.load_or_new(&wrong_network).is_err());
            let mut snapshot = store.load_or_new(&network).expect("reload");
            assert!(apply_observation(
                &mut snapshot.journal,
                &txid,
                ReconcileObservation::Confirmed,
            )
            .expect("confirm"));
            store
                .save_next(snapshot.generation, &snapshot.journal)
                .expect("save confirmed");
        }

        {
            let store = WalletPendingJournalStore::try_acquire(&path).expect("second reopen");
            let mut snapshot = store.load_or_new(&network).expect("second reload");
            let entry = snapshot.journal.entry(&txid).expect("entry");
            assert_eq!(entry.state, WalletPendingState::Confirmed);
            assert!(!entry.state.reserves_outpoints());
            assert_eq!(snapshot.generation, 2);
            assert!(!apply_observation(
                &mut snapshot.journal,
                &txid,
                ReconcileObservation::Confirmed,
            )
            .expect("repeat confirmation"));
        }

        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn public_transport_is_https_or_loopback_only() {
        assert!(relay_base_url("https://relay.example").is_ok());
        assert!(relay_base_url("http://127.0.0.1:8080").is_ok());
        assert!(relay_base_url("http://localhost:8080").is_ok());
        assert!(relay_base_url("http://relay.example").is_err());
        assert!(relay_base_url("https://user:pass@relay.example").is_err());
        assert!(relay_base_url("https://relay.example/path").is_err());
    }
}
