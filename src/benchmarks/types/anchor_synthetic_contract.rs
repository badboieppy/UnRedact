use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const ANCHOR_SYNTHETIC_CONFIG_JSON: &str =
    include_str!("../contracts/anchor_synthetic_config.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorSyntheticContract {
    pub contract_id: String,
    pub schema_version: usize,
    pub seeds: Vec<u64>,
    pub samples_per_pdf: usize,
    pub max_pdfs: usize,
    pub min_gap_pt: f32,
    pub max_gap_pt: f32,
    pub max_center_y_delta_pt: f32,
}

fn validate_contract(contract: &AnchorSyntheticContract) -> Result<(), String> {
    if contract.contract_id.trim().is_empty() {
        return Err("anchor synthetic config has empty contract_id".to_owned());
    }
    if contract.seeds.is_empty() {
        return Err("anchor synthetic config has no seeds".to_owned());
    }
    if contract.samples_per_pdf == 0 {
        return Err("anchor synthetic config samples_per_pdf must be > 0".to_owned());
    }
    if !contract.min_gap_pt.is_finite() || contract.min_gap_pt < 0.0_f32 {
        return Err("anchor synthetic config min_gap_pt must be finite and >= 0".to_owned());
    }
    if !contract.max_gap_pt.is_finite() || contract.max_gap_pt <= contract.min_gap_pt {
        return Err("anchor synthetic config max_gap_pt must be > min_gap_pt".to_owned());
    }
    if !contract.max_center_y_delta_pt.is_finite() || contract.max_center_y_delta_pt < 0.0_f32 {
        return Err(
            "anchor synthetic config max_center_y_delta_pt must be finite and >= 0".to_owned(),
        );
    }
    Ok(())
}

#[inline]
pub fn canonical_anchor_synthetic_contract() -> Result<&'static AnchorSyntheticContract, String> {
    static ANCHOR_SYNTHETIC_CONTRACT: OnceLock<AnchorSyntheticContract> = OnceLock::new();
    if let Some(contract) = ANCHOR_SYNTHETIC_CONTRACT.get() {
        return Ok(contract);
    }
    let parsed = serde_json::from_str::<AnchorSyntheticContract>(ANCHOR_SYNTHETIC_CONFIG_JSON)
        .map_err(|error| format!("failed to parse anchor synthetic config: {error}"))?;
    validate_contract(&parsed)?;
    let _ignored = ANCHOR_SYNTHETIC_CONTRACT.set(parsed);
    ANCHOR_SYNTHETIC_CONTRACT
        .get()
        .ok_or_else(|| "anchor synthetic config cache was not initialized after parse".to_owned())
}

#[cfg(test)]
mod tests {
    use super::canonical_anchor_synthetic_contract;

    #[test]
    fn canonical_config_has_expected_shape() {
        let contract =
            canonical_anchor_synthetic_contract().expect("anchor synthetic config should parse");
        assert_eq!(contract.contract_id, "C-ANCHOR-SYNTHETIC-CONFIG-V1");
        assert!(!contract.seeds.is_empty());
        assert!(contract.samples_per_pdf > 0);
    }
}
