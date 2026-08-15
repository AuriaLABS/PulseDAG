use std::{error::Error, fmt, net::IpAddr, time::Duration};

use pulsedag_core::{compute_txid, types::Transaction};
use pulsedag_wallet::{WalletNetworkIdentity, WalletReviewSummary};
use reqwest::{redirect::Policy, Client, Response, Url};
use serde::{Deserialize, Serialize};

const RELAY_HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const RELAY_RESPONSE_MAX_BYTES: usize = 1024 * 1024;
const RELAY_VERSION: &str = "signed-transaction-relay-v1";
const RELAY_CAPABILITY: &str = "signed_transaction_relay";
const RELAY_SUBMIT_PATH: &str = "/api/v1/tx/submit";

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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedBroadcastInput {
    pub network: WalletNetworkIdentity,
    pub review: WalletReviewSummary,
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

#[derive(Debug, Clone)]
struct RelayIdentity {
    network: WalletNetworkIdentity,
    version: String,
}

pub fn parse_signed_broadcast(bytes: &[u8]) -> Result<SignedBroadcastInput, RelayClientError> {
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

fn validate_release_identity(
    signed_network: &WalletNetworkIdentity,
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
    signed_network
        .ensure_matches(&observed)
        .map_err(|error| relay_error(format!("relay network mismatch: {error}")))?;
    if data.signed_transaction_relay_version != RELAY_VERSION {
        return Err(relay_error(format!(
            "unsupported relay version: {}",
            data.signed_transaction_relay_version
        )));
    }
    if !data
        .capabilities
        .iter()
        .any(|value| value == RELAY_CAPABILITY)
    {
        return Err(relay_error(
            "relay identity is missing signed transaction capability",
        ));
    }
    if !data
        .core_endpoints
        .iter()
        .any(|value| value == RELAY_SUBMIT_PATH)
    {
        return Err(relay_error(
            "relay identity does not advertise canonical submit endpoint",
        ));
    }
    Ok(RelayIdentity {
        network: observed,
        version: data.signed_transaction_relay_version,
    })
}

async fn fetch_identity(
    client: &Client,
    base: &Url,
    signed_network: &WalletNetworkIdentity,
) -> Result<RelayIdentity, RelayClientError> {
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
    let parsed = serde_json::from_slice::<ApiResponse<ReleaseIdentityData>>(&body)
        .map_err(|_| relay_error("relay identity response JSON is invalid"))?;
    validate_release_identity(signed_network, parsed)
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
            review: WalletReviewSummary {
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
    fn relay_url_requires_https_except_loopback() {
        assert!(relay_base_url("https://relay.example").is_ok());
        assert!(relay_base_url("http://127.0.0.1:8080").is_ok());
        assert!(relay_base_url("http://localhost:8080").is_ok());
        assert!(relay_base_url("http://relay.example").is_err());
        assert!(relay_base_url("https://user:pass@relay.example").is_err());
        assert!(relay_base_url("https://relay.example/path").is_err());
    }
}
