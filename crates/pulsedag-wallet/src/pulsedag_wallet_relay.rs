use std::{error::Error, fmt, net::IpAddr, time::Duration};

use pulsedag_core::{
    compute_txid,
    types::{Transaction, Utxo},
};
use pulsedag_wallet::WalletNetworkIdentity;
use reqwest::{redirect::Policy, Client, Response, Url};
use serde::{Deserialize, Serialize};

const RELAY_HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const RELAY_RESPONSE_MAX_BYTES: usize = 1024 * 1024;
const RELAY_VERSION: &str = "signed-transaction-relay-v1";
const RELAY_CAPABILITY: &str = "signed_transaction_relay";
const RELAY_SUBMIT_PATH: &str = "/api/v1/tx/submit";
const EXPLORER_CAPABILITY: &str = "explorer_api";
const ADDRESS_PATH: &str = "/address/:address";
const ADDRESS_UTXOS_PATH: &str = "/address/:address/utxos";
const SAFETY_REVIEW_FIELDS: [&str; 7] = [
    "self_send",
    "spend_all",
    "self_send_acknowledged",
    "spend_all_acknowledged",
    "funding_utxo_count",
    "funding_total_amount",
    "funding_snapshot_commitment_hex",
];
const HIGH_FEE_REVIEW_FIELDS: [&str; 2] = ["high_fee", "high_fee_acknowledged"];

#[derive(Debug)]
pub struct RelayClientError(String);

impl fmt::Display for RelayClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for RelayClientError {}

fn relay_error(message: impl Into<String>) -> RelayClientError {
    RelayClientError(message.into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelayEnvelope {
    pub transaction: Transaction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedBroadcastReview {
    pub network_profile: String,
    pub chain_id: String,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub fee: u64,
    pub change: u64,
    pub total_input: u64,
    pub input_count: usize,
    pub nonce: u64,
    pub unsigned_template_txid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_send: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spend_all: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_send_acknowledged: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spend_all_acknowledged: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub funding_utxo_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub funding_total_amount: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub funding_snapshot_commitment_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub high_fee: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub high_fee_acknowledged: Option<bool>,
}

impl SignedBroadcastReview {
    fn validate_safety_metadata_shape(&self) -> Result<(), RelayClientError> {
        let safety_present = [
            self.self_send.is_some(),
            self.spend_all.is_some(),
            self.self_send_acknowledged.is_some(),
            self.spend_all_acknowledged.is_some(),
            self.funding_utxo_count.is_some(),
            self.funding_total_amount.is_some(),
            self.funding_snapshot_commitment_hex.is_some(),
        ];
        let high_fee_present = [
            self.high_fee.is_some(),
            self.high_fee_acknowledged.is_some(),
        ];
        let safety_count = safety_present.iter().filter(|value| **value).count();
        let high_fee_count = high_fee_present.iter().filter(|value| **value).count();
        match (safety_count, high_fee_count) {
            (0, 0) | (7, 0) | (7, 2) => Ok(()),
            _ => Err(relay_error(
                "signed envelope review safety metadata must use an accepted complete 0, 7, or 9 field shape",
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedBroadcastInput {
    pub network: WalletNetworkIdentity,
    pub review: SignedBroadcastReview,
    pub final_txid: String,
    pub relay: RelayEnvelope,
}

#[derive(Debug, Serialize)]
pub struct BroadcastOutput {
    pub accepted: bool,
    pub txid: String,
    pub mempool_size: Option<usize>,
    pub relay_network_profile: String,
    pub relay_chain_id: String,
    pub relay_version: String,
    pub rejection_code: Option<String>,
    pub rejection_message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AddressBalanceOutput {
    pub network_profile: String,
    pub chain_id: String,
    pub address: String,
    pub confirmed_balance: u64,
    pub confirmed_utxo_count: usize,
    pub largest_utxo: u64,
}

#[derive(Debug, Serialize)]
pub struct AddressUtxosOutput {
    pub network_profile: String,
    pub chain_id: String,
    pub address: String,
    pub count: usize,
    pub utxos: Vec<Utxo>,
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
    signed_transaction_relay_version: String,
    capabilities: Vec<String>,
    core_endpoints: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SubmitData {
    accepted: bool,
    txid: String,
    mempool_size: usize,
}

#[derive(Debug, Deserialize)]
struct AddressBalanceData {
    address: String,
    confirmed_balance: u64,
    confirmed_utxo_count: usize,
    largest_utxo: u64,
}

#[derive(Debug, Deserialize)]
struct AddressUtxosData {
    address: String,
    count: usize,
    utxos: Vec<Utxo>,
}

#[derive(Debug, Clone)]
struct RelayIdentity {
    network: WalletNetworkIdentity,
    version: String,
    capabilities: Vec<String>,
    core_endpoints: Vec<String>,
}

fn validate_raw_safety_metadata_shape(bytes: &[u8]) -> Result<(), RelayClientError> {
    let value = serde_json::from_slice::<serde_json::Value>(bytes)
        .map_err(|_| relay_error("signed transaction envelope JSON is invalid"))?;
    let review = value
        .get("review")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| relay_error("signed transaction envelope JSON is invalid"))?;

    let mut safety_count = 0;
    for field in SAFETY_REVIEW_FIELDS {
        if let Some(value) = review.get(field) {
            safety_count += 1;
            if value.is_null() {
                return Err(relay_error(
                    "signed envelope review safety metadata must not contain null values",
                ));
            }
        }
    }
    let mut high_fee_count = 0;
    for field in HIGH_FEE_REVIEW_FIELDS {
        if let Some(value) = review.get(field) {
            high_fee_count += 1;
            if value.is_null() {
                return Err(relay_error(
                    "signed envelope review safety metadata must not contain null values",
                ));
            }
        }
    }
    match (safety_count, high_fee_count) {
        (0, 0) | (7, 0) | (7, 2) => Ok(()),
        _ => Err(relay_error(
            "signed envelope review safety metadata must use an accepted complete 0, 7, or 9 field shape",
        )),
    }
}

pub fn parse_signed_broadcast(bytes: &[u8]) -> Result<SignedBroadcastInput, RelayClientError> {
    validate_raw_safety_metadata_shape(bytes)?;
    let input = serde_json::from_slice::<SignedBroadcastInput>(bytes)
        .map_err(|_| relay_error("signed transaction envelope JSON is invalid"))?;
    validate_signed_broadcast(&input)?;
    Ok(input)
}

fn validate_signed_broadcast(input: &SignedBroadcastInput) -> Result<(), RelayClientError> {
    input
        .network
        .validate()
        .map_err(|error| relay_error(format!("signed envelope network is invalid: {error}")))?;
    input.review.validate_safety_metadata_shape()?;

    if input.review.network_profile != input.network.network_profile
        || input.review.chain_id != input.network.chain_id
    {
        return Err(relay_error(
            "signed envelope review network does not match signed network metadata",
        ));
    }

    let transaction = &input.relay.transaction;
    if transaction.inputs.is_empty() {
        return Err(relay_error("signed transaction has no inputs"));
    }
    for input in &transaction.inputs {
        let public_key = hex::decode(&input.public_key)
            .map_err(|_| relay_error("signed transaction contains an invalid public key"))?;
        if public_key.len() != 32 {
            return Err(relay_error(
                "signed transaction public key must be 32 bytes",
            ));
        }
        let signature = hex::decode(&input.signature)
            .map_err(|_| relay_error("signed transaction contains an invalid signature"))?;
        if signature.len() != 64 {
            return Err(relay_error("signed transaction signature must be 64 bytes"));
        }
    }

    let canonical_txid = compute_txid(transaction);
    if transaction.txid != canonical_txid {
        return Err(relay_error(
            "signed transaction txid does not match canonical transaction bytes",
        ));
    }
    if input.final_txid != canonical_txid {
        return Err(relay_error(
            "signed envelope final_txid does not match canonical transaction txid",
        ));
    }
    Ok(())
}

fn relay_base_url(raw: &str) -> Result<Url, RelayClientError> {
    let mut url = Url::parse(raw).map_err(|_| relay_error("relay URL is invalid"))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(relay_error("relay URL must not contain credentials"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(relay_error("relay URL must not contain query or fragment"));
    }
    if !matches!(url.path(), "" | "/") {
        return Err(relay_error("relay URL must be an origin without a path"));
    }

    let host = url
        .host_str()
        .ok_or_else(|| relay_error("relay URL must contain a host"))?;
    match url.scheme() {
        "https" => {}
        "http" if is_loopback_host(host) => {}
        "http" => {
            return Err(relay_error(
                "plain HTTP relay URLs are allowed only for loopback development",
            ));
        }
        _ => {
            return Err(relay_error(
                "relay URL scheme must be https or loopback http",
            ))
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

fn build_client() -> Result<Client, RelayClientError> {
    Client::builder()
        .timeout(RELAY_HTTP_TIMEOUT)
        .redirect(Policy::none())
        .build()
        .map_err(|error| relay_error(format!("failed to initialize relay HTTP client: {error}")))
}

async fn bounded_body(
    mut response: Response,
) -> Result<(reqwest::StatusCode, Vec<u8>), RelayClientError> {
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > RELAY_RESPONSE_MAX_BYTES as u64)
    {
        return Err(relay_error("relay response exceeds body limit"));
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| relay_error(format!("failed reading relay response: {error}")))?
    {
        if bytes.len().saturating_add(chunk.len()) > RELAY_RESPONSE_MAX_BYTES {
            return Err(relay_error("relay response exceeds body limit"));
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

fn validate_remote_identity(
    expected_network: &WalletNetworkIdentity,
    response: ApiResponse<ReleaseIdentityData>,
) -> Result<RelayIdentity, RelayClientError> {
    if !response.ok {
        return Err(relay_error(format!(
            "relay identity request failed: {}",
            api_error_detail(response.error)
        )));
    }
    let data = response
        .data
        .ok_or_else(|| relay_error("relay identity response is missing data"))?;
    let observed = WalletNetworkIdentity::new(data.network_profile, data.chain_id)
        .map_err(|error| relay_error(format!("relay identity is invalid: {error}")))?;
    expected_network
        .ensure_matches(&observed)
        .map_err(|error| relay_error(format!("relay network mismatch: {error}")))?;
    Ok(RelayIdentity {
        network: observed,
        version: data.signed_transaction_relay_version,
        capabilities: data.capabilities,
        core_endpoints: data.core_endpoints,
    })
}

fn require_surface(
    identity: &RelayIdentity,
    capability: &str,
    endpoint: &str,
) -> Result<(), RelayClientError> {
    if !identity
        .capabilities
        .iter()
        .any(|value| value == capability)
    {
        return Err(relay_error(format!(
            "relay identity is missing required capability: {capability}"
        )));
    }
    if !identity
        .core_endpoints
        .iter()
        .any(|value| value == endpoint)
    {
        return Err(relay_error(format!(
            "relay identity does not advertise canonical endpoint: {endpoint}"
        )));
    }
    Ok(())
}

fn validate_release_identity(
    signed_network: &WalletNetworkIdentity,
    response: ApiResponse<ReleaseIdentityData>,
) -> Result<RelayIdentity, RelayClientError> {
    let identity = validate_remote_identity(signed_network, response)?;
    if identity.version != RELAY_VERSION {
        return Err(relay_error(format!(
            "unsupported relay version: {}",
            identity.version
        )));
    }
    require_surface(&identity, RELAY_CAPABILITY, RELAY_SUBMIT_PATH)?;
    Ok(identity)
}

fn validate_explorer_identity(
    expected_network: &WalletNetworkIdentity,
    response: ApiResponse<ReleaseIdentityData>,
    endpoint: &str,
) -> Result<RelayIdentity, RelayClientError> {
    let identity = validate_remote_identity(expected_network, response)?;
    require_surface(&identity, EXPLORER_CAPABILITY, endpoint)?;
    Ok(identity)
}

async fn fetch_identity_response(
    client: &Client,
    base: &Url,
) -> Result<ApiResponse<ReleaseIdentityData>, RelayClientError> {
    let url = base
        .join("release")
        .map_err(|_| relay_error("failed to construct relay identity URL"))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| relay_error(format!("relay identity transport failed: {error}")))?;
    let (status, body) = bounded_body(response).await?;
    if !status.is_success() {
        return Err(relay_error(format!(
            "relay identity request returned HTTP {}",
            status.as_u16()
        )));
    }
    serde_json::from_slice::<ApiResponse<ReleaseIdentityData>>(&body)
        .map_err(|_| relay_error("relay identity response JSON is invalid"))
}

async fn fetch_identity(
    client: &Client,
    base: &Url,
    signed_network: &WalletNetworkIdentity,
) -> Result<RelayIdentity, RelayClientError> {
    validate_release_identity(signed_network, fetch_identity_response(client, base).await?)
}

async fn fetch_explorer_identity(
    client: &Client,
    base: &Url,
    expected_network: &WalletNetworkIdentity,
    endpoint: &str,
) -> Result<RelayIdentity, RelayClientError> {
    validate_explorer_identity(
        expected_network,
        fetch_identity_response(client, base).await?,
        endpoint,
    )
}

async fn submit_transaction(
    client: &Client,
    base: &Url,
    signed: &SignedBroadcastInput,
    identity: &RelayIdentity,
) -> Result<BroadcastOutput, RelayClientError> {
    let url = base
        .join("api/v1/tx/submit")
        .map_err(|_| relay_error("failed to construct relay submit URL"))?;
    let response = client
        .post(url)
        .json(&signed.relay)
        .send()
        .await
        .map_err(|error| relay_error(format!("relay submit transport failed: {error}")))?;
    let (status, body) = bounded_body(response).await?;
    let parsed = serde_json::from_slice::<ApiResponse<SubmitData>>(&body).map_err(|_| {
        relay_error(format!(
            "relay submit response JSON is invalid (HTTP {})",
            status.as_u16()
        ))
    })?;

    if !parsed.ok {
        let error = parsed.error.unwrap_or(ApiError {
            code: format!("HTTP_{}", status.as_u16()),
            message: "relay rejected transaction without an error body".to_string(),
        });
        return Ok(BroadcastOutput {
            accepted: false,
            txid: signed.final_txid.clone(),
            mempool_size: None,
            relay_network_profile: identity.network.network_profile.clone(),
            relay_chain_id: identity.network.chain_id.clone(),
            relay_version: identity.version.clone(),
            rejection_code: Some(error.code),
            rejection_message: Some(error.message),
        });
    }
    if !status.is_success() {
        return Err(relay_error(format!(
            "relay returned successful API payload with HTTP {}",
            status.as_u16()
        )));
    }
    let data = parsed
        .data
        .ok_or_else(|| relay_error("relay submit response is missing data"))?;
    if !data.accepted {
        return Err(relay_error("relay returned ok=true with accepted=false"));
    }
    if data.txid != signed.final_txid {
        return Err(relay_error("relay accepted a different transaction id"));
    }

    Ok(BroadcastOutput {
        accepted: true,
        txid: data.txid,
        mempool_size: Some(data.mempool_size),
        relay_network_profile: identity.network.network_profile.clone(),
        relay_chain_id: identity.network.chain_id.clone(),
        relay_version: identity.version.clone(),
        rejection_code: None,
        rejection_message: None,
    })
}

fn address_url(base: &Url, address: &str, include_utxos: bool) -> Result<Url, RelayClientError> {
    if address.is_empty() || address.trim() != address {
        return Err(relay_error("wallet address is invalid"));
    }
    let mut url = base.clone();
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| relay_error("relay URL cannot contain path segments"))?;
        segments.pop_if_empty();
        segments.push("address");
        segments.push(address);
        if include_utxos {
            segments.push("utxos");
        }
    }
    Ok(url)
}

pub async fn fetch_address_balance(
    relay_url: &str,
    expected_network: &WalletNetworkIdentity,
    address: &str,
) -> Result<AddressBalanceOutput, RelayClientError> {
    expected_network
        .validate()
        .map_err(|error| relay_error(format!("wallet network is invalid: {error}")))?;
    let base = relay_base_url(relay_url)?;
    let client = build_client()?;
    let identity = fetch_explorer_identity(&client, &base, expected_network, ADDRESS_PATH).await?;
    let response = client
        .get(address_url(&base, address, false)?)
        .send()
        .await
        .map_err(|error| relay_error(format!("address balance transport failed: {error}")))?;
    let (status, body) = bounded_body(response).await?;
    if !status.is_success() {
        return Err(relay_error(format!(
            "address balance request returned HTTP {}",
            status.as_u16()
        )));
    }
    let parsed = serde_json::from_slice::<ApiResponse<AddressBalanceData>>(&body)
        .map_err(|_| relay_error("address balance response JSON is invalid"))?;
    if !parsed.ok {
        return Err(relay_error(format!(
            "address balance request failed: {}",
            api_error_detail(parsed.error)
        )));
    }
    let data = parsed
        .data
        .ok_or_else(|| relay_error("address balance response is missing data"))?;
    if data.address != address {
        return Err(relay_error("address balance response address mismatch"));
    }
    Ok(AddressBalanceOutput {
        network_profile: identity.network.network_profile,
        chain_id: identity.network.chain_id,
        address: data.address,
        confirmed_balance: data.confirmed_balance,
        confirmed_utxo_count: data.confirmed_utxo_count,
        largest_utxo: data.largest_utxo,
    })
}

pub async fn fetch_address_utxos(
    relay_url: &str,
    expected_network: &WalletNetworkIdentity,
    address: &str,
) -> Result<AddressUtxosOutput, RelayClientError> {
    expected_network
        .validate()
        .map_err(|error| relay_error(format!("wallet network is invalid: {error}")))?;
    let base = relay_base_url(relay_url)?;
    let client = build_client()?;
    let identity =
        fetch_explorer_identity(&client, &base, expected_network, ADDRESS_UTXOS_PATH).await?;
    let response = client
        .get(address_url(&base, address, true)?)
        .send()
        .await
        .map_err(|error| relay_error(format!("address UTXO transport failed: {error}")))?;
    let (status, body) = bounded_body(response).await?;
    if !status.is_success() {
        return Err(relay_error(format!(
            "address UTXO request returned HTTP {}",
            status.as_u16()
        )));
    }
    let parsed = serde_json::from_slice::<ApiResponse<AddressUtxosData>>(&body)
        .map_err(|_| relay_error("address UTXO response JSON is invalid"))?;
    if !parsed.ok {
        return Err(relay_error(format!(
            "address UTXO request failed: {}",
            api_error_detail(parsed.error)
        )));
    }
    let data = parsed
        .data
        .ok_or_else(|| relay_error("address UTXO response is missing data"))?;
    if data.address != address {
        return Err(relay_error("address UTXO response address mismatch"));
    }
    if data.count != data.utxos.len() {
        return Err(relay_error("address UTXO response count mismatch"));
    }
    if data.utxos.iter().any(|utxo| utxo.address != address) {
        return Err(relay_error(
            "address UTXO response contains a different address",
        ));
    }
    Ok(AddressUtxosOutput {
        network_profile: identity.network.network_profile,
        chain_id: identity.network.chain_id,
        address: data.address,
        count: data.count,
        utxos: data.utxos,
    })
}

pub async fn broadcast_signed(
    relay_url: &str,
    signed: SignedBroadcastInput,
) -> Result<BroadcastOutput, RelayClientError> {
    validate_signed_broadcast(&signed)?;
    let base = relay_base_url(relay_url)?;
    let client = build_client()?;
    let identity = fetch_identity(&client, &base, &signed.network).await?;
    submit_transaction(&client, &base, &signed, &identity).await
}

#[cfg(test)]
mod tests {
    use pulsedag_core::types::{OutPoint, TxInput, TxOutput};

    use super::*;

    fn signed_fixture() -> SignedBroadcastInput {
        let network = WalletNetworkIdentity::new("testnet", "pulsedag-testnet").unwrap();
        let mut transaction = Transaction {
            txid: String::new(),
            version: 1,
            inputs: vec![TxInput {
                previous_output: OutPoint {
                    txid: "11".repeat(32),
                    index: 0,
                },
                public_key: "22".repeat(32),
                signature: "33".repeat(64),
            }],
            outputs: vec![TxOutput {
                address: "pulse1recipient".to_string(),
                amount: 400,
            }],
            fee: 10,
            nonce: 7,
        };
        transaction.txid = compute_txid(&transaction);
        SignedBroadcastInput {
            network,
            review: SignedBroadcastReview {
                network_profile: "testnet".to_string(),
                chain_id: "pulsedag-testnet".to_string(),
                from: "pulse1sender".to_string(),
                to: "pulse1recipient".to_string(),
                amount: 400,
                fee: 10,
                change: 0,
                total_input: 410,
                input_count: 1,
                nonce: 7,
                unsigned_template_txid: "44".repeat(32),
                self_send: Some(false),
                spend_all: Some(true),
                self_send_acknowledged: Some(false),
                spend_all_acknowledged: Some(true),
                funding_utxo_count: Some(1),
                funding_total_amount: Some(410),
                funding_snapshot_commitment_hex: Some(
                    "60ddbcbc5857a0f66bf7aeaecce2edacc2a1f46f9d7d443c305a997d01d15aea".to_string(),
                ),
                high_fee: Some(true),
                high_fee_acknowledged: Some(true),
            },
            final_txid: transaction.txid.clone(),
            relay: RelayEnvelope { transaction },
        }
    }

    fn identity_response(
        network_profile: &str,
        chain_id: &str,
    ) -> ApiResponse<ReleaseIdentityData> {
        ApiResponse {
            ok: true,
            data: Some(ReleaseIdentityData {
                network_profile: network_profile.to_string(),
                chain_id: chain_id.to_string(),
                signed_transaction_relay_version: RELAY_VERSION.to_string(),
                capabilities: vec![RELAY_CAPABILITY.to_string()],
                core_endpoints: vec![RELAY_SUBMIT_PATH.to_string()],
            }),
            error: None,
        }
    }

    fn explorer_identity_response(
        network_profile: &str,
        chain_id: &str,
        endpoint: &str,
    ) -> ApiResponse<ReleaseIdentityData> {
        ApiResponse {
            ok: true,
            data: Some(ReleaseIdentityData {
                network_profile: network_profile.to_string(),
                chain_id: chain_id.to_string(),
                signed_transaction_relay_version: RELAY_VERSION.to_string(),
                capabilities: vec![EXPLORER_CAPABILITY.to_string()],
                core_endpoints: vec![endpoint.to_string()],
            }),
            error: None,
        }
    }

    #[test]
    fn signed_envelope_parser_accepts_exact_0_7_and_9_field_safety_shapes() {
        let current = serde_json::to_vec(&signed_fixture()).unwrap();
        let parsed_current = parse_signed_broadcast(&current).unwrap();
        assert_eq!(parsed_current.review.spend_all, Some(true));
        assert_eq!(parsed_current.review.funding_utxo_count, Some(1));
        assert_eq!(parsed_current.review.high_fee, Some(true));
        assert_eq!(parsed_current.review.high_fee_acknowledged, Some(true));

        let mut old_safety = serde_json::to_value(signed_fixture()).unwrap();
        let review = old_safety
            .get_mut("review")
            .and_then(serde_json::Value::as_object_mut)
            .unwrap();
        for field in HIGH_FEE_REVIEW_FIELDS {
            review.remove(field);
        }
        let parsed_old =
            parse_signed_broadcast(&serde_json::to_vec(&old_safety).unwrap()).unwrap();
        assert_eq!(parsed_old.review.spend_all, Some(true));
        assert!(parsed_old.review.high_fee.is_none());
        assert!(parsed_old.review.high_fee_acknowledged.is_none());

        let mut legacy = old_safety;
        let review = legacy
            .get_mut("review")
            .and_then(serde_json::Value::as_object_mut)
            .unwrap();
        for field in SAFETY_REVIEW_FIELDS {
            review.remove(field);
        }
        let parsed_legacy = parse_signed_broadcast(&serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert!(parsed_legacy.review.self_send.is_none());
        assert!(parsed_legacy
            .review
            .funding_snapshot_commitment_hex
            .is_none());
        assert!(parsed_legacy.review.high_fee.is_none());
    }

    #[test]
    fn signed_envelope_parser_rejects_partial_or_mixed_safety_review_metadata() {
        let mut missing_old = serde_json::to_value(signed_fixture()).unwrap();
        missing_old
            .get_mut("review")
            .and_then(serde_json::Value::as_object_mut)
            .unwrap()
            .remove("funding_snapshot_commitment_hex");
        assert!(parse_signed_broadcast(&serde_json::to_vec(&missing_old).unwrap()).is_err());

        let mut missing_high = serde_json::to_value(signed_fixture()).unwrap();
        missing_high
            .get_mut("review")
            .and_then(serde_json::Value::as_object_mut)
            .unwrap()
            .remove("high_fee_acknowledged");
        assert!(parse_signed_broadcast(&serde_json::to_vec(&missing_high).unwrap()).is_err());

        let mut high_without_old = serde_json::to_value(signed_fixture()).unwrap();
        let review = high_without_old
            .get_mut("review")
            .and_then(serde_json::Value::as_object_mut)
            .unwrap();
        for field in SAFETY_REVIEW_FIELDS {
            review.remove(field);
        }
        assert!(parse_signed_broadcast(&serde_json::to_vec(&high_without_old).unwrap()).is_err());
    }

    #[test]
    fn signed_envelope_parser_rejects_null_safety_review_metadata() {
        let mut old_null = serde_json::to_value(signed_fixture()).unwrap();
        old_null
            .get_mut("review")
            .and_then(serde_json::Value::as_object_mut)
            .unwrap()
            .insert("self_send".to_string(), serde_json::Value::Null);
        assert!(parse_signed_broadcast(&serde_json::to_vec(&old_null).unwrap()).is_err());

        let mut high_null = serde_json::to_value(signed_fixture()).unwrap();
        high_null
            .get_mut("review")
            .and_then(serde_json::Value::as_object_mut)
            .unwrap()
            .insert("high_fee".to_string(), serde_json::Value::Null);
        assert!(parse_signed_broadcast(&serde_json::to_vec(&high_null).unwrap()).is_err());
    }

    #[test]
    fn signed_envelope_requires_canonical_final_txid() {
        let mut signed = signed_fixture();
        validate_signed_broadcast(&signed).unwrap();
        signed.final_txid = "00".repeat(32);
        assert!(validate_signed_broadcast(&signed).is_err());
    }

    #[test]
    fn signed_envelope_requires_signature_material() {
        let mut signed = signed_fixture();
        signed.relay.transaction.inputs[0].signature.clear();
        signed.relay.transaction.txid = compute_txid(&signed.relay.transaction);
        signed.final_txid = signed.relay.transaction.txid.clone();
        assert!(validate_signed_broadcast(&signed).is_err());
    }

    #[test]
    fn relay_identity_must_match_signed_network_and_capability() {
        let signed = signed_fixture();
        assert!(validate_release_identity(
            &signed.network,
            identity_response("testnet", "pulsedag-testnet")
        )
        .is_ok());
        assert!(validate_release_identity(
            &signed.network,
            identity_response("testnet", "other-chain")
        )
        .is_err());

        let mut missing_capability = identity_response("testnet", "pulsedag-testnet");
        missing_capability
            .data
            .as_mut()
            .unwrap()
            .capabilities
            .clear();
        assert!(validate_release_identity(&signed.network, missing_capability).is_err());
    }

    #[test]
    fn explorer_identity_requires_exact_network_capability_and_endpoint() {
        let network = WalletNetworkIdentity::new("testnet", "pulsedag-testnet").unwrap();
        assert!(validate_explorer_identity(
            &network,
            explorer_identity_response("testnet", "pulsedag-testnet", ADDRESS_PATH),
            ADDRESS_PATH,
        )
        .is_ok());
        assert!(validate_explorer_identity(
            &network,
            explorer_identity_response("testnet", "other-chain", ADDRESS_PATH),
            ADDRESS_PATH,
        )
        .is_err());
        assert!(validate_explorer_identity(
            &network,
            explorer_identity_response("testnet", "pulsedag-testnet", ADDRESS_UTXOS_PATH),
            ADDRESS_PATH,
        )
        .is_err());

        let mut no_capability =
            explorer_identity_response("testnet", "pulsedag-testnet", ADDRESS_PATH);
        no_capability.data.as_mut().unwrap().capabilities.clear();
        assert!(validate_explorer_identity(&network, no_capability, ADDRESS_PATH).is_err());
    }

    #[test]
    fn address_url_encodes_address_as_single_path_segment() {
        let base = relay_base_url("https://relay.example").unwrap();
        let url = address_url(&base, "pulse/../release?x#y", true).unwrap();
        let segments = url.path_segments().unwrap().collect::<Vec<_>>();
        assert_eq!(
            segments,
            vec!["address", "pulse%2F..%2Frelease%3Fx%23y", "utxos"]
        );
        assert!(url.query().is_none());
        assert!(url.fragment().is_none());
        assert!(!url.as_str().contains("/release?"));
    }

    #[test]
    fn relay_url_requires_https_except_loopback() {
        assert!(relay_base_url("https://relay.example").is_ok());
        assert!(relay_base_url("http://127.0.0.1:8080").is_ok());
        assert!(relay_base_url("http://localhost:8080").is_ok());
        assert!(relay_base_url("http://relay.example").is_err());
        assert!(relay_base_url("https://user:pass@relay.example").is_err());
        assert!(relay_base_url("https://relay.example/path").is_err());
    }
}
