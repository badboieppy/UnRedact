use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::OnceLock;

const KNOWN_REDACTION_CONTRACT_JSON: &str =
    include_str!("../contracts/known_redaction_targets.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownRedactionContract {
    pub contract_id: String,
    pub schema_version: usize,
    pub datasets: Vec<KnownRedactionDataset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownRedactionDataset {
    pub name: String,
    pub input_pdf: String,
    pub row_selector: KnownRedactionRowSelector,
    pub targets: Vec<KnownRedactionTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KnownRedactionRowSelector {
    PageYRange {
        page_index: u32,
        y0_min: f32,
        y1_max: f32,
    },
    PositionFromEnd {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownRedactionTarget {
    pub label: String,
    pub target: String,
    pub selector: KnownRedactionTargetSelector,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KnownRedactionTargetSelector {
    InPool {},
    IndexFromEnd { index_from_end: usize },
}

impl KnownRedactionContract {
    #[inline]
    pub fn canonical_target_count(&self) -> usize {
        self.datasets
            .iter()
            .map(|dataset| dataset.targets.len())
            .sum::<usize>()
    }

    #[inline]
    pub fn dataset_by_name(&self, name: &str) -> Option<&KnownRedactionDataset> {
        self.datasets.iter().find(|dataset| dataset.name == name)
    }
}

fn validate_contract(contract: &KnownRedactionContract) -> Result<(), String> {
    if contract.contract_id.trim().is_empty() {
        return Err("known redaction contract has empty contract_id".to_owned());
    }
    if contract.datasets.is_empty() {
        return Err("known redaction contract has no datasets".to_owned());
    }
    let mut names = BTreeSet::<String>::new();
    for dataset in &contract.datasets {
        if dataset.name.trim().is_empty() {
            return Err("known redaction contract contains dataset with empty name".to_owned());
        }
        if !names.insert(dataset.name.clone()) {
            return Err(format!(
                "known redaction contract contains duplicate dataset '{}'",
                dataset.name
            ));
        }
        if dataset.input_pdf.trim().is_empty() {
            return Err(format!(
                "known redaction contract dataset '{}' has empty input_pdf",
                dataset.name
            ));
        }
        if dataset.targets.is_empty() {
            return Err(format!(
                "known redaction contract dataset '{}' has no targets",
                dataset.name
            ));
        }
        for target in &dataset.targets {
            if target.label.trim().is_empty() || target.target.trim().is_empty() {
                return Err(format!(
                    "known redaction contract dataset '{}' has target with empty label/target",
                    dataset.name
                ));
            }
            if matches!(
                target.selector,
                KnownRedactionTargetSelector::IndexFromEnd { index_from_end: 0 }
            ) {
                return Err(format!(
                    "known redaction contract dataset '{}' has index_from_end=0",
                    dataset.name
                ));
            }
        }
    }
    Ok(())
}

#[inline]
pub fn canonical_known_redaction_contract() -> Result<&'static KnownRedactionContract, String> {
    static KNOWN_REDACTION_CONTRACT: OnceLock<KnownRedactionContract> = OnceLock::new();
    if let Some(contract) = KNOWN_REDACTION_CONTRACT.get() {
        return Ok(contract);
    }
    let parsed = serde_json::from_str::<KnownRedactionContract>(KNOWN_REDACTION_CONTRACT_JSON)
        .map_err(|error| format!("failed to parse known redaction contract: {error}"))?;
    validate_contract(&parsed)?;
    let _ignored = KNOWN_REDACTION_CONTRACT.set(parsed);
    KNOWN_REDACTION_CONTRACT
        .get()
        .ok_or_else(|| "known redaction contract cache was not initialized after parse".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_known_redaction_contract, KnownRedactionRowSelector, KnownRedactionTargetSelector,
    };

    #[test]
    fn canonical_contract_contains_expected_known_target_set_shape() {
        let contract =
            canonical_known_redaction_contract().expect("contract should parse and validate");
        assert_eq!(contract.contract_id, "C-KNOWN-REDACTION-TARGETS-V1");
        assert_eq!(contract.canonical_target_count(), 11);

        let efta00038617 = contract
            .dataset_by_name("EFTA00038617")
            .expect("EFTA00038617 dataset should exist");
        assert_eq!(efta00038617.targets.len(), 9);
        assert!(matches!(
            efta00038617.row_selector,
            KnownRedactionRowSelector::PageYRange { .. }
        ));
        assert!(efta00038617
            .targets
            .iter()
            .all(|target| matches!(target.selector, KnownRedactionTargetSelector::InPool {})));

        let efta00101126 = contract
            .dataset_by_name("EFTA00101126")
            .expect("EFTA00101126 dataset should exist");
        assert_eq!(efta00101126.targets.len(), 2);
        assert!(matches!(
            efta00101126.row_selector,
            KnownRedactionRowSelector::PositionFromEnd {}
        ));
        assert!(efta00101126.targets.iter().all(|target| matches!(
            target.selector,
            KnownRedactionTargetSelector::IndexFromEnd {
                index_from_end: 1 | 2
            }
        )));
    }
}
