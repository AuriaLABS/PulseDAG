use pulsedag_core::types::BlockHeader;
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const OFFICIAL_POW_FIXTURE_RELATIVE_PATH: &str = "../../fixtures/pow/official_vectors.json";

#[derive(Debug, Deserialize)]
pub struct VectorFixture {
    pub schema_version: u32,
    pub algorithm: String,
    pub valid_vectors: Vec<FixtureVector>,
    pub invalid_vectors: Vec<InvalidFixtureVector>,
}

#[derive(Debug, Deserialize)]
pub struct FixtureVector {
    pub id: String,
    pub header: BlockHeader,
    pub expected: ExpectedPow,
}

#[derive(Debug, Deserialize)]
pub struct InvalidFixtureVector {
    pub id: String,
    pub header: BlockHeader,
    pub expected: ExpectedPow,
    pub must_fail_fields: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExpectedPow {
    pub preimage_hex: String,
    pub pow_hash_hex: String,
    pub pow_score_u64: u64,
    pub target_u64: u64,
    pub accepts: bool,
}

#[derive(Copy, Clone, Debug)]
pub enum PowVectorField {
    PreimageHex,
    PowHashHex,
    PowScoreU64,
    TargetU64,
    Accepts,
}

impl PowVectorField {
    pub fn parse(field_name: &str) -> Option<Self> {
        match field_name {
            "preimage_hex" => Some(Self::PreimageHex),
            "pow_hash_hex" => Some(Self::PowHashHex),
            "pow_score_u64" => Some(Self::PowScoreU64),
            "target_u64" => Some(Self::TargetU64),
            "accepts" => Some(Self::Accepts),
            _ => None,
        }
    }
}

pub fn load_official_fixture(manifest_dir: &str) -> VectorFixture {
    let manifest_dir = Path::new(manifest_dir);
    let fixture_path = fixture_path_from_manifest(manifest_dir);
    let body = std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", fixture_path.display()));
    serde_json::from_str::<VectorFixture>(&body).expect("official pow fixture must parse")
}

pub fn fixture_path_from_manifest(manifest_dir: &Path) -> PathBuf {
    manifest_dir.join(OFFICIAL_POW_FIXTURE_RELATIVE_PATH)
}
