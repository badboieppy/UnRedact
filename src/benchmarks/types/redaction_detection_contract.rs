use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::OnceLock;

const REDACTION_DETECTION_CONTRACT_JSON: &str =
    include_str!("../contracts/redaction_detection_targets.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionDetectionContract {
    pub contract_id: String,
    pub schema_version: usize,
    pub datasets: Vec<RedactionDetectionDataset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionDetectionDataset {
    pub name: String,
    pub input_pdf: String,
    pub ground_truth_redactions: String,
}

impl RedactionDetectionContract {
    #[inline]
    pub fn dataset_by_name(&self, name: &str) -> Option<&RedactionDetectionDataset> {
        self.datasets.iter().find(|dataset| dataset.name == name)
    }
}

fn validate_contract(contract: &RedactionDetectionContract) -> Result<(), String> {
    if contract.contract_id.trim().is_empty() {
        return Err("redaction detection contract has empty contract_id".to_owned());
    }
    if contract.datasets.is_empty() {
        return Err("redaction detection contract has no datasets".to_owned());
    }
    let mut names = BTreeSet::<String>::new();
    for dataset in &contract.datasets {
        if dataset.name.trim().is_empty() {
            return Err("redaction detection contract has dataset with empty name".to_owned());
        }
        if !names.insert(dataset.name.clone()) {
            return Err(format!(
                "redaction detection contract has duplicate dataset '{}'",
                dataset.name
            ));
        }
        if dataset.input_pdf.trim().is_empty() {
            return Err(format!(
                "redaction detection contract dataset '{}' has empty input_pdf",
                dataset.name
            ));
        }
        if dataset.ground_truth_redactions.trim().is_empty() {
            return Err(format!(
                "redaction detection contract dataset '{}' has empty ground_truth_redactions",
                dataset.name
            ));
        }
    }
    Ok(())
}

#[inline]
pub fn canonical_redaction_detection_contract(
) -> Result<&'static RedactionDetectionContract, String> {
    static REDACTION_DETECTION_CONTRACT: OnceLock<RedactionDetectionContract> = OnceLock::new();
    if let Some(contract) = REDACTION_DETECTION_CONTRACT.get() {
        return Ok(contract);
    }
    let parsed =
        serde_json::from_str::<RedactionDetectionContract>(REDACTION_DETECTION_CONTRACT_JSON)
            .map_err(|error| format!("failed to parse redaction detection contract: {error}"))?;
    validate_contract(&parsed)?;
    let _ignored = REDACTION_DETECTION_CONTRACT.set(parsed);
    REDACTION_DETECTION_CONTRACT.get().ok_or_else(|| {
        "redaction detection contract cache was not initialized after parse".to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::canonical_redaction_detection_contract;

    #[test]
    fn canonical_contract_has_expected_shape() {
        let contract = canonical_redaction_detection_contract()
            .expect("redaction detection contract should parse");
        assert_eq!(contract.contract_id, "C-REDACTION-DETECTION-TARGETS-V1");
        assert!(contract.dataset_by_name("EFTA00038617").is_some());
        assert!(contract.dataset_by_name("EFTA00101126").is_some());
    }
}
