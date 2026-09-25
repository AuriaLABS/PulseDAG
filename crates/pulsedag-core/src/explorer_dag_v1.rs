//! Planning matcher for explorer DAG view v1.
//!
//! Official 3.0 surface must show parents, selected parent, blue/red,
//! and PulseClock when present. A linear hash list is not enough.
//! Default admission keeps this out of the shipped explorer.

use crate::pulseclock_v1::PulseObservationV1;

pub const EXPLORER_DAG_DOMAIN_V1: &str = "PulseDAG:explorer-dag:v1";
pub const EXPLORER_DAG_VERSION_V1: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerMergeColorV1 {
    Selected,
    Blue,
    Red,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerBlockV1 {
    pub hash: String,
    pub parents: Vec<String>,
    pub selected_parent: Option<String>,
    pub merge_color: ExplorerMergeColorV1,
    pub blue_score: u64,
    pub pulse: Option<PulseObservationV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplorerDagAdmissionV1 {
    pub surface_enabled: bool,
}

impl ExplorerDagAdmissionV1 {
    pub const INACTIVE: Self = Self {
        surface_enabled: false,
    };

    pub fn admitted(self) -> bool {
        self.surface_enabled
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplorerDagV1Error {
    SurfaceDisabled,
    EmptyHash,
    LinearOnly,
    MissingSelectedParent,
    EmptyParentHash,
}

impl std::fmt::Display for ExplorerDagV1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurfaceDisabled => {
                write!(f, "explorer DAG view is not admitted on this candidate")
            }
            Self::EmptyHash => write!(f, "explorer block hash must not be empty"),
            Self::LinearOnly => {
                write!(
                    f,
                    "linear hash list is not a DAG view; parents are required"
                )
            }
            Self::MissingSelectedParent => {
                write!(f, "non-genesis explorer block must name selected_parent")
            }
            Self::EmptyParentHash => write!(f, "parent hash must not be empty"),
        }
    }
}

impl std::error::Error for ExplorerDagV1Error {}

pub fn is_genesis_view_v1(block: &ExplorerBlockV1) -> bool {
    block.parents.is_empty() && block.selected_parent.is_none() && block.blue_score == 0
}

pub fn validate_explorer_block_v1(
    admission: ExplorerDagAdmissionV1,
    block: &ExplorerBlockV1,
) -> Result<(), ExplorerDagV1Error> {
    if !admission.admitted() {
        return Err(ExplorerDagV1Error::SurfaceDisabled);
    }
    if block.hash.is_empty() {
        return Err(ExplorerDagV1Error::EmptyHash);
    }
    if block.parents.iter().any(|parent| parent.is_empty()) {
        return Err(ExplorerDagV1Error::EmptyParentHash);
    }
    if is_genesis_view_v1(block) {
        return Ok(());
    }
    if block.parents.is_empty() {
        return Err(ExplorerDagV1Error::LinearOnly);
    }
    match &block.selected_parent {
        Some(parent) if !parent.is_empty() => Ok(()),
        _ => Err(ExplorerDagV1Error::MissingSelectedParent),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pulseclock_v1::{PULSE_DOMAIN_V1, PULSE_VERSION_V1, PULSE_WINDOW_K_V1};

    fn admitted() -> ExplorerDagAdmissionV1 {
        ExplorerDagAdmissionV1 {
            surface_enabled: true,
        }
    }

    fn child() -> ExplorerBlockV1 {
        ExplorerBlockV1 {
            hash: "b1".into(),
            parents: vec!["g".into()],
            selected_parent: Some("g".into()),
            merge_color: ExplorerMergeColorV1::Selected,
            blue_score: 1,
            pulse: Some(PulseObservationV1 {
                pulse_version: PULSE_VERSION_V1,
                domain: PULSE_DOMAIN_V1,
                chain_id: "exp".into(),
                selected_tip: "b1".into(),
                pulse_height: 1,
                pulse_time: 1_700_000_000,
                window_k: PULSE_WINDOW_K_V1,
                sample_count: 1,
                uncertainty_secs: 1,
                finality_lag: 1,
            }),
        }
    }

    #[test]
    fn inactive_surface_rejects_even_a_full_dag_row() {
        assert_eq!(
            validate_explorer_block_v1(ExplorerDagAdmissionV1::INACTIVE, &child()),
            Err(ExplorerDagV1Error::SurfaceDisabled)
        );
    }

    #[test]
    fn linear_hash_list_is_rejected() {
        let mut block = child();
        block.parents.clear();
        block.selected_parent = Some("g".into());
        block.blue_score = 1;
        assert_eq!(
            validate_explorer_block_v1(admitted(), &block),
            Err(ExplorerDagV1Error::LinearOnly)
        );
    }

    #[test]
    fn non_genesis_needs_selected_parent() {
        let mut block = child();
        block.selected_parent = None;
        assert_eq!(
            validate_explorer_block_v1(admitted(), &block),
            Err(ExplorerDagV1Error::MissingSelectedParent)
        );
    }

    #[test]
    fn genesis_without_parents_is_accepted() {
        let genesis = ExplorerBlockV1 {
            hash: "g".into(),
            parents: vec![],
            selected_parent: None,
            merge_color: ExplorerMergeColorV1::Selected,
            blue_score: 0,
            pulse: None,
        };
        validate_explorer_block_v1(admitted(), &genesis).unwrap();
        validate_explorer_block_v1(admitted(), &child()).unwrap();
    }
}
