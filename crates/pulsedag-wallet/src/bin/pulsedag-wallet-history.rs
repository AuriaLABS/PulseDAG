use std::{
    collections::HashMap,
    env,
    error::Error,
    fmt, fs,
    io::{self, Write},
    net::IpAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use pulsedag_wallet::{
    WalletDerivationBranch, WalletNetworkIdentity, WalletWatchOnly, WalletWatchOnlyBranch,
    WalletWatchOnlyManifest,
};
use reqwest::{redirect::Policy, Client, Response, Url};
use serde::{Deserialize, Serialize};

const JSON_INPUT_MAX_BYTES: u64 = 4 * 1024 * 1024;
const RESPONSE_MAX_BYTES: usize = 1024 * 1024;
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const EXPLORER_CAPABILITY: &str = "explorer_api";
const ACTIVITY_ENDPOINT: &str = "/address/:address/activity";
const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 100;

type CliResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug)]
struct HistoryArgs {
    manifest: PathBuf,
    branch: WalletDerivationBranch,
    index: u32,
    relay: String,
    limit: usize,
    offset: usize,
}

#[derive(Debug)]
struct HistoryClientError(String);

impl fmt::Display for HistoryClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for HistoryClientError {}

fn history_error(message: impl Into<String>) -> HistoryClientError {
    HistoryClientError(message.into())
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

#[derive(Debug, Serialize)]
struct HistoryOutput {
    network_profile: String,
    chain_id: String,
    address: String,
    count: usize,
    total: usize,
    limit: usize,
    offset: usize,
    has_more: bool,
    history_scope: &'static str,
    activity: Vec<AddressActivityItem>,
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

fn parse_u32(name: &str, value: &str) -> CliResult<u32> {
    value
        .parse::<u32>()
        .map_err(|_| invalid_input(format!("{name} must be an unsigned 32-bit integer")).into())
}

fn parse_usize(name: &str, value: &str) -> CliResult<usize> {
    value
        .parse::<usize>()
        .map_err(|_| invalid_input(format!("{name} must be a non-negative integer")).into())
}

fn parse_branch(value: &str) -> CliResult<WalletDerivationBranch> {
    match value {
        "receive" => Ok(WalletDerivationBranch::Receive),
        "change" => Ok(WalletDerivationBranch::Change),
        _ => Err(invalid_input("--branch must be receive or change").into()),
    }
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

fn parse_args_from(args: impl Iterator<Item = String>) -> CliResult<HistoryArgs> {
    let flags = parse_flags(args)?;
    for key in flags.keys() {
        if !["manifest", "branch", "index", "relay", "limit", "offset"]
            .iter()
            .any(|allowed| key == allowed)
        {
            return Err(invalid_input(format!("unknown option --{key}")).into());
        }
    }
    let limit = flags
        .get("limit")
        .map(|value| parse_usize("--limit", value))
        .transpose()?
        .unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return Err(invalid_input(format!("--limit must be between 1 and {MAX_LIMIT}")).into());
    }
    let offset = flags
        .get("offset")
        .map(|value| parse_usize("--offset", value))
        .transpose()?
        .unwrap_or(0);
    Ok(HistoryArgs {
        manifest: PathBuf::from(required(&flags, "manifest")?),
        branch: parse_branch(&required(&flags, "branch")?)?,
        index: parse_u32("--index", &required(&flags, "index")?)?,
        relay: required(&flags, "relay")?,
        limit,
        offset,
    })
}

fn parse_args() -> CliResult<HistoryArgs> {
    parse_args_from(env::args().skip(1))
}

fn read_manifest(path: &Path) -> CliResult<WalletWatchOnlyManifest> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(invalid_input("watch-only manifest must be a regular file").into());
    }
    if metadata.len() > JSON_INPUT_MAX_BYTES {
        return Err(invalid_input(format!(
            "watch-only manifest exceeds {JSON_INPUT_MAX_BYTES} byte input limit"
        ))
        .into());
    }
    let manifest = serde_json::from_slice::<WalletWatchOnlyManifest>(&fs::read(path)?)
        .map_err(|_| invalid_input("watch-only manifest JSON is invalid"))?;
    manifest.validate()?;
    Ok(manifest)
}

fn selected_watch_target(args: &HistoryArgs) -> CliResult<(WalletNetworkIdentity, String)> {
    let watch_only = WalletWatchOnly::import(read_manifest(&args.manifest)?)?;
    let expected_branch = match args.branch {
        WalletDerivationBranch::Receive => WalletWatchOnlyBranch::Receive,
        WalletDerivationBranch::Change => WalletWatchOnlyBranch::Change,
    };
    let address = watch_only
        .entries()
        .iter()
        .find(|entry| entry.branch() == expected_branch && entry.index() == args.index)
        .ok_or_else(|| invalid_input("selected watch-only address is not present in manifest"))?
        .address()
        .to_string();
    let network = WalletNetworkIdentity::new(watch_only.network_profile(), watch_only.chain_id())?;
    Ok((network, address))
}

fn relay_base_url(raw: &str) -> Result<Url, HistoryClientError> {
    let mut url = Url::parse(raw).map_err(|_| history_error("relay URL is invalid"))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(history_error("relay URL must not contain credentials"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(history_error(
            "relay URL must not contain query or fragment",
        ));
    }
    if !matches!(url.path(), "" | "/") {
        return Err(history_error("relay URL must be an origin without a path"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| history_error("relay URL must contain a host"))?;
    match url.scheme() {
        "https" => {}
        "http" if is_loopback_host(host) => {}
        "http" => {
            return Err(history_error(
                "plain HTTP relay URLs are allowed only for loopback development",
            ));
        }
        _ => {
            return Err(history_error(
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

fn build_client() -> Result<Client, HistoryClientError> {
    Client::builder()
        .timeout(HTTP_TIMEOUT)
        .redirect(Policy::none())
        .build()
        .map_err(|error| history_error(format!("failed to initialize relay HTTP client: {error}")))
}

async fn bounded_body(
    mut response: Response,
) -> Result<(reqwest::StatusCode, Vec<u8>), HistoryClientError> {
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > RESPONSE_MAX_BYTES as u64)
    {
        return Err(history_error("relay response exceeds body limit"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| history_error(format!("failed reading relay response: {error}")))?
    {
        if bytes.len().saturating_add(chunk.len()) > RESPONSE_MAX_BYTES {
            return Err(history_error("relay response exceeds body limit"));
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
) -> Result<WalletNetworkIdentity, HistoryClientError> {
    if !response.ok {
        return Err(history_error(format!(
            "relay identity request failed: {}",
            api_error_detail(response.error)
        )));
    }
    let data = response
        .data
        .ok_or_else(|| history_error("relay identity response is missing data"))?;
    let observed = WalletNetworkIdentity::new(data.network_profile, data.chain_id)
        .map_err(|error| history_error(format!("relay identity is invalid: {error}")))?;
    expected
        .ensure_matches(&observed)
        .map_err(|error| history_error(format!("relay network mismatch: {error}")))?;
    if !data
        .capabilities
        .iter()
        .any(|capability| capability == EXPLORER_CAPABILITY)
    {
        return Err(history_error(
            "relay identity is missing required capability: explorer_api",
        ));
    }
    if !data
        .core_endpoints
        .iter()
        .any(|endpoint| endpoint == ACTIVITY_ENDPOINT)
    {
        return Err(history_error(format!(
            "relay identity does not advertise canonical endpoint: {ACTIVITY_ENDPOINT}"
        )));
    }
    Ok(observed)
}

async fn fetch_release_identity(
    client: &Client,
    base: &Url,
    expected: &WalletNetworkIdentity,
) -> Result<WalletNetworkIdentity, HistoryClientError> {
    let url = base
        .join("release")
        .map_err(|_| history_error("failed to construct relay identity URL"))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| history_error(format!("relay identity transport failed: {error}")))?;
    let (status, body) = bounded_body(response).await?;
    if !status.is_success() {
        return Err(history_error(format!(
            "relay identity request returned HTTP {}",
            status.as_u16()
        )));
    }
    let parsed = serde_json::from_slice::<ApiResponse<ReleaseIdentityData>>(&body)
        .map_err(|_| history_error("relay identity response JSON is invalid"))?;
    validate_release_identity(expected, parsed)
}

fn activity_url(
    base: &Url,
    address: &str,
    limit: usize,
    offset: usize,
) -> Result<Url, HistoryClientError> {
    if address.is_empty() || address.trim() != address {
        return Err(history_error("wallet address is invalid"));
    }
    let mut url = base.clone();
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| history_error("relay URL is not a valid base URL"))?;
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

fn validate_activity_item(item: &AddressActivityItem) -> Result<(), HistoryClientError> {
    if item.txid.is_empty() || item.txid.trim() != item.txid {
        return Err(history_error("activity item contains invalid txid"));
    }
    if item.direction != expected_direction(item.net) {
        return Err(history_error("activity item direction/net mismatch"));
    }
    let expected_net = i128::from(item.incoming) - i128::from(item.outgoing);
    if expected_net != i128::from(item.net) {
        return Err(history_error("activity item amount/net mismatch"));
    }
    match item.context.as_str() {
        "mempool" => {
            if !item.is_mempool
                || item.is_confirmed
                || item.block_hash.is_some()
                || item.block_height.is_some()
            {
                return Err(history_error("mempool activity state is incoherent"));
            }
        }
        "confirmed" => {
            if item.is_mempool
                || !item.is_confirmed
                || item.block_hash.as_deref().is_none_or(str::is_empty)
                || item.block_height.is_none()
            {
                return Err(history_error("confirmed activity state is incoherent"));
            }
        }
        _ => return Err(history_error("activity item context is unsupported")),
    }
    Ok(())
}

fn validate_activity_response(
    address: &str,
    requested_limit: usize,
    requested_offset: usize,
    data: AddressActivityData,
) -> Result<AddressActivityData, HistoryClientError> {
    if data.address != address {
        return Err(history_error("address activity response address mismatch"));
    }
    if data.limit != requested_limit || data.offset != requested_offset {
        return Err(history_error("address activity pagination mismatch"));
    }
    if data.count != data.activity.len() || data.count > data.limit {
        return Err(history_error("address activity response count mismatch"));
    }
    if data.count == 0 {
        if data.offset < data.total {
            return Err(history_error("address activity empty-page total/offset mismatch"));
        }
    } else if data.offset >= data.total || data.offset.saturating_add(data.count) > data.total {
        return Err(history_error("address activity total/offset mismatch"));
    }
    let expected_has_more = data.offset.saturating_add(data.count) < data.total;
    if data.has_more != expected_has_more {
        return Err(history_error("address activity has_more mismatch"));
    }
    for item in &data.activity {
        validate_activity_item(item)?;
    }
    Ok(data)
}

async fn fetch_history(
    relay: &str,
    expected_network: &WalletNetworkIdentity,
    address: &str,
    limit: usize,
    offset: usize,
) -> Result<HistoryOutput, HistoryClientError> {
    expected_network
        .validate()
        .map_err(|error| history_error(format!("wallet network is invalid: {error}")))?;
    let base = relay_base_url(relay)?;
    let client = build_client()?;
    let observed = fetch_release_identity(&client, &base, expected_network).await?;
    let response = client
        .get(activity_url(&base, address, limit, offset)?)
        .send()
        .await
        .map_err(|error| history_error(format!("address activity transport failed: {error}")))?;
    let (status, body) = bounded_body(response).await?;
    if !status.is_success() {
        return Err(history_error(format!(
            "address activity request returned HTTP {}",
            status.as_u16()
        )));
    }
    let parsed = serde_json::from_slice::<ApiResponse<AddressActivityData>>(&body)
        .map_err(|_| history_error("address activity response JSON is invalid"))?;
    if !parsed.ok {
        return Err(history_error(format!(
            "address activity request failed: {}",
            api_error_detail(parsed.error)
        )));
    }
    let data = validate_activity_response(
        address,
        limit,
        offset,
        parsed
            .data
            .ok_or_else(|| history_error("address activity response is missing data"))?,
    )?;
    Ok(HistoryOutput {
        network_profile: observed.network_profile,
        chain_id: observed.chain_id,
        address: data.address,
        count: data.count,
        total: data.total,
        limit: data.limit,
        offset: data.offset,
        has_more: data.has_more,
        history_scope: "retained_node_history",
        activity: data.activity,
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
        let (network, address) = selected_watch_target(&args)?;
        let output =
            fetch_history(&args.relay, &network, &address, args.limit, args.offset).await?;
        write_json(&output)
    }
    .await;
    if let Err(error) = result {
        eprintln!("pulsedag-wallet-history: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> impl Iterator<Item = String> {
        values
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn item(context: &str, net: i64) -> AddressActivityItem {
        let (incoming, outgoing) = if net > 0 {
            (net as u64, 0)
        } else if net < 0 {
            (0, net.unsigned_abs())
        } else {
            (5, 5)
        };
        AddressActivityItem {
            txid: "11".repeat(32),
            direction: expected_direction(net).to_string(),
            incoming,
            outgoing,
            net,
            context: context.to_string(),
            is_mempool: context == "mempool",
            is_confirmed: context == "confirmed",
            block_hash: (context == "confirmed").then(|| "22".repeat(32)),
            block_height: (context == "confirmed").then_some(7),
        }
    }

    #[test]
    fn parser_accepts_bounded_public_history_options_only() {
        let parsed = parse_args_from(args(&[
            "--manifest",
            "watch.json",
            "--branch",
            "receive",
            "--index",
            "0",
            "--relay",
            "https://relay.example",
            "--limit",
            "50",
            "--offset",
            "100",
        ]))
        .expect("history args");
        assert_eq!(parsed.limit, 50);
        assert_eq!(parsed.offset, 100);
        assert!(parse_args_from(args(&[
            "--manifest",
            "watch.json",
            "--branch",
            "receive",
            "--index",
            "0",
            "--relay",
            "https://relay.example",
            "--password",
            "secret",
        ]))
        .is_err());
        assert!(parse_args_from(args(&[
            "--manifest",
            "watch.json",
            "--branch",
            "receive",
            "--index",
            "0",
            "--relay",
            "https://relay.example",
            "--limit",
            "101",
        ]))
        .is_err());
    }

    #[test]
    fn activity_url_keeps_address_in_one_path_segment() {
        let base = relay_base_url("https://relay.example").expect("base URL");
        let url = activity_url(&base, "pulse/../release?x#y", 20, 3).expect("activity URL");
        let segments = url.path_segments().unwrap().collect::<Vec<_>>();
        assert_eq!(
            segments,
            vec!["address", "pulse%2F..%2Frelease%3Fx%23y", "activity"]
        );
        let query = url.query_pairs().collect::<HashMap<_, _>>();
        assert_eq!(query.get("limit").map(|v| v.as_ref()), Some("20"));
        assert_eq!(query.get("offset").map(|v| v.as_ref()), Some("3"));
    }

    #[test]
    fn public_transport_rejects_plain_http_and_credentials() {
        assert!(relay_base_url("https://relay.example").is_ok());
        assert!(relay_base_url("http://127.0.0.1:8080").is_ok());
        assert!(relay_base_url("http://localhost:8080").is_ok());
        assert!(relay_base_url("http://relay.example").is_err());
        assert!(relay_base_url("https://user:pass@relay.example").is_err());
        assert!(relay_base_url("https://relay.example/path").is_err());
    }

    #[test]
    fn release_identity_requires_network_capability_and_activity_endpoint() {
        let expected = WalletNetworkIdentity::new("testnet", "pulsedag-testnet").unwrap();
        let response = ApiResponse {
            ok: true,
            data: Some(ReleaseIdentityData {
                network_profile: "testnet".to_string(),
                chain_id: "pulsedag-testnet".to_string(),
                capabilities: vec![EXPLORER_CAPABILITY.to_string()],
                core_endpoints: vec![ACTIVITY_ENDPOINT.to_string()],
            }),
            error: None,
        };
        assert!(validate_release_identity(&expected, response).is_ok());

        let wrong_network = ApiResponse {
            ok: true,
            data: Some(ReleaseIdentityData {
                network_profile: "mainnet".to_string(),
                chain_id: "pulsedag-mainnet".to_string(),
                capabilities: vec![EXPLORER_CAPABILITY.to_string()],
                core_endpoints: vec![ACTIVITY_ENDPOINT.to_string()],
            }),
            error: None,
        };
        assert!(validate_release_identity(&expected, wrong_network).is_err());
    }

    #[test]
    fn activity_response_fails_closed_on_state_or_pagination_incoherence() {
        let valid = AddressActivityData {
            address: "pulse1sender".to_string(),
            count: 2,
            total: 3,
            limit: 2,
            offset: 0,
            has_more: true,
            activity: vec![item("mempool", -5), item("confirmed", 8)],
        };
        assert!(validate_activity_response("pulse1sender", 2, 0, valid).is_ok());

        let empty_beyond_end = AddressActivityData {
            address: "pulse1sender".to_string(),
            count: 0,
            total: 3,
            limit: 2,
            offset: 10,
            has_more: false,
            activity: Vec::new(),
        };
        assert!(validate_activity_response("pulse1sender", 2, 10, empty_beyond_end).is_ok());

        let bad_empty_page = AddressActivityData {
            address: "pulse1sender".to_string(),
            count: 0,
            total: 3,
            limit: 2,
            offset: 1,
            has_more: true,
            activity: Vec::new(),
        };
        assert!(validate_activity_response("pulse1sender", 2, 1, bad_empty_page).is_err());

        let bad_state = AddressActivityData {
            address: "pulse1sender".to_string(),
            count: 1,
            total: 1,
            limit: 2,
            offset: 0,
            has_more: false,
            activity: vec![AddressActivityItem {
                is_confirmed: true,
                ..item("mempool", -5)
            }],
        };
        assert!(validate_activity_response("pulse1sender", 2, 0, bad_state).is_err());

        let bad_pagination = AddressActivityData {
            address: "pulse1sender".to_string(),
            count: 1,
            total: 1,
            limit: 2,
            offset: 0,
            has_more: true,
            activity: vec![item("confirmed", 8)],
        };
        assert!(validate_activity_response("pulse1sender", 2, 0, bad_pagination).is_err());
    }
}
