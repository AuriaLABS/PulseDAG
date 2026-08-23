use pulsedag_api::ApiResponse;
use reqwest::Client;
use serde::Deserialize;
use tokio::time::{sleep, Duration};

pub const SUBMIT_FINALITY_UNKNOWN_CODE: &str = "submit_finality_unknown";
pub const RECONCILIATION_ATTEMPTS: u32 = 20;
pub const RECONCILIATION_BACKOFF_MS: u64 = 500;
const RECONCILIATION_REQUEST_TIMEOUT_SECS: u64 = 2;

#[derive(Debug, Deserialize)]
struct BlockLookupData {
    hash: String,
    height: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconciliationOutcome {
    Accepted { height: Option<u64> },
    Rejected { reason_code: String, reason: String },
    StillUnknown { detail: String },
}

fn classify_block_lookup(
    expected_hash: &str,
    data: Option<&BlockLookupData>,
    error_code: Option<&str>,
    error_message: Option<&str>,
) -> Option<ReconciliationOutcome> {
    if let Some(block) = data {
        if block.hash == expected_hash {
            return Some(ReconciliationOutcome::Accepted {
                height: Some(block.height),
            });
        }
    }

    match error_code.unwrap_or_default().to_ascii_lowercase().as_str() {
        "block_rejected" | "rejected" | "invalid_block" => Some(ReconciliationOutcome::Rejected {
            reason_code: error_code.unwrap_or("block_rejected").to_ascii_lowercase(),
            reason: error_message
                .unwrap_or("node reported a definitive rejected block outcome")
                .to_string(),
        }),
        _ => None,
    }
}

pub async fn reconcile_submit_finality(
    client: &Client,
    node: &str,
    block_hash: &str,
) -> ReconciliationOutcome {
    let lookup_url = format!("{}/blocks/{}", node.trim_end_matches('/'), block_hash);
    let mut last_detail = "block lookup has not completed".to_string();

    for attempt in 1..=RECONCILIATION_ATTEMPTS {
        match client
            .get(&lookup_url)
            .timeout(Duration::from_secs(RECONCILIATION_REQUEST_TIMEOUT_SECS))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                match response.json::<ApiResponse<BlockLookupData>>().await {
                    Ok(api) => {
                        let error_code = api.error.as_ref().map(|error| error.code.as_str());
                        let error_message = api.error.as_ref().map(|error| error.message.as_str());
                        if let Some(outcome) = classify_block_lookup(
                            block_hash,
                            api.data.as_ref(),
                            error_code,
                            error_message,
                        ) {
                            return outcome;
                        }
                        last_detail = format!(
                            "attempt {attempt}: block not present and node exposed no definitive rejection"
                        );
                    }
                    Err(error) => {
                        last_detail = format!(
                            "attempt {attempt}: block lookup response could not be decoded: {error}"
                        );
                    }
                }
            }
            Ok(response) => {
                last_detail = format!(
                    "attempt {attempt}: block lookup returned HTTP {}",
                    response.status()
                );
            }
            Err(error) => {
                last_detail = format!("attempt {attempt}: block lookup failed: {error}");
            }
        }

        if attempt < RECONCILIATION_ATTEMPTS {
            sleep(Duration::from_millis(RECONCILIATION_BACKOFF_MS)).await;
        }
    }

    ReconciliationOutcome::StillUnknown {
        detail: last_detail,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_block_lookup, BlockLookupData, ReconciliationOutcome, SUBMIT_FINALITY_UNKNOWN_CODE,
    };

    #[test]
    fn task29_matching_block_hash_reconciles_to_accepted() {
        let block = BlockLookupData {
            hash: "abc".to_string(),
            height: 7,
        };
        assert_eq!(
            classify_block_lookup("abc", Some(&block), None, None),
            Some(ReconciliationOutcome::Accepted { height: Some(7) })
        );
    }

    #[test]
    fn task29_not_found_lookup_remains_non_final() {
        assert_eq!(
            classify_block_lookup("abc", None, Some("NOT_FOUND"), Some("block not found")),
            None
        );
    }

    #[test]
    fn task29_explicit_rejected_lookup_is_definitive() {
        assert_eq!(
            classify_block_lookup(
                "abc",
                None,
                Some("BLOCK_REJECTED"),
                Some("definitive rejection")
            ),
            Some(ReconciliationOutcome::Rejected {
                reason_code: "block_rejected".to_string(),
                reason: "definitive rejection".to_string(),
            })
        );
    }

    #[test]
    fn task29_finality_unknown_code_is_stable() {
        assert_eq!(SUBMIT_FINALITY_UNKNOWN_CODE, "submit_finality_unknown");
    }
}
