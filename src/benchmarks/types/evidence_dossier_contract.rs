use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const INVESTIGATIVE_PROOF_CONTRACT_ID: &str = "C-INVESTIGATIVE-PROOF-FIRST";
pub const BEST_FEASIBLE_K_CONTRACT_ID: &str = "C-BEST-FEASIBLE-K-RULE";
pub const BASELINE_NO_REGRESSION_CONTRACT_ID: &str = "C-BASELINE-NO-REGRESSION";
pub const INVESTIGATIVE_PROOF_SCHEMA_VERSION: usize = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceDossier {
    pub contract_id: String,
    pub schema_version: usize,
    pub proposal_id: String,
    pub proposal_title: String,
    pub proposal_owner: String,
    pub created_at_utc: String,
    pub consumed_contracts: Vec<String>,
    pub baseline_artifacts: BaselineArtifacts,
    pub baseline_metrics: Vec<BaselineMetric>,
    pub hypothesis: Hypothesis,
    pub expected_metric_deltas: Vec<ExpectedMetricDelta>,
    pub disconfirmation_signals: Vec<DisconfirmationSignal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineArtifacts {
    pub known_baseline_path: String,
    pub deterministic_gate_path: String,
    pub synthetic_gate_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineMetric {
    pub metric_id: String,
    pub value: f64,
    pub source_artifact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hypothesis {
    pub mechanism: String,
    pub expected_effect: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeltaDirection {
    Increase,
    Decrease,
    NotLower,
    NotHigher,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedMetricDelta {
    pub metric_id: String,
    pub direction: DeltaDirection,
    pub minimum_delta: Option<f64>,
    pub maximum_delta: Option<f64>,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Comparator {
    GreaterThan,
    GreaterOrEqual,
    LessThan,
    LessOrEqual,
    Equal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconfirmationSignal {
    pub signal_id: String,
    pub metric_id: String,
    pub comparator: Comparator,
    pub threshold: f64,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DossierValidationSummary {
    pub completeness_passed: bool,
    pub contract_alignment_passed: bool,
    pub disconfirmation_passed: bool,
    pub errors: Vec<String>,
}

#[inline]
pub fn validate_evidence_dossier(dossier: &EvidenceDossier) -> DossierValidationSummary {
    let mut completeness_errors = Vec::<String>::new();
    let mut contract_errors = Vec::<String>::new();
    let mut disconfirmation_errors = Vec::<String>::new();

    if dossier.contract_id.trim().is_empty() {
        completeness_errors.push("missing contract_id".to_owned());
    }
    if dossier.schema_version == 0 {
        completeness_errors.push("schema_version must be > 0".to_owned());
    }
    if dossier.proposal_id.trim().is_empty() {
        completeness_errors.push("missing proposal_id".to_owned());
    }
    if dossier.proposal_title.trim().is_empty() {
        completeness_errors.push("missing proposal_title".to_owned());
    }
    if dossier.proposal_owner.trim().is_empty() {
        completeness_errors.push("missing proposal_owner".to_owned());
    }
    if dossier.created_at_utc.trim().is_empty() {
        completeness_errors.push("missing created_at_utc".to_owned());
    }
    if dossier.hypothesis.mechanism.trim().is_empty() {
        completeness_errors.push("missing hypothesis.mechanism".to_owned());
    }
    if dossier.hypothesis.expected_effect.trim().is_empty() {
        completeness_errors.push("missing hypothesis.expected_effect".to_owned());
    }
    if dossier
        .baseline_artifacts
        .known_baseline_path
        .trim()
        .is_empty()
    {
        completeness_errors.push("missing baseline_artifacts.known_baseline_path".to_owned());
    }
    if dossier
        .baseline_artifacts
        .deterministic_gate_path
        .trim()
        .is_empty()
    {
        completeness_errors.push("missing baseline_artifacts.deterministic_gate_path".to_owned());
    }
    if dossier
        .baseline_artifacts
        .synthetic_gate_path
        .trim()
        .is_empty()
    {
        completeness_errors.push("missing baseline_artifacts.synthetic_gate_path".to_owned());
    }
    if dossier.baseline_metrics.is_empty() {
        completeness_errors.push("baseline_metrics must contain at least one metric".to_owned());
    }
    if dossier.expected_metric_deltas.is_empty() {
        completeness_errors.push("expected_metric_deltas must not be empty".to_owned());
    }
    if dossier.disconfirmation_signals.is_empty() {
        completeness_errors.push("disconfirmation_signals must not be empty".to_owned());
    }

    if dossier.contract_id != INVESTIGATIVE_PROOF_CONTRACT_ID {
        contract_errors.push(format!(
            "contract_id must equal {INVESTIGATIVE_PROOF_CONTRACT_ID}"
        ));
    }
    if dossier.schema_version != INVESTIGATIVE_PROOF_SCHEMA_VERSION {
        contract_errors.push(format!(
            "schema_version must equal {INVESTIGATIVE_PROOF_SCHEMA_VERSION}"
        ));
    }

    let consumed = dossier
        .consumed_contracts
        .iter()
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
        .collect::<BTreeSet<_>>();
    if !consumed.contains(BEST_FEASIBLE_K_CONTRACT_ID) {
        contract_errors.push(format!(
            "consumed_contracts must include {BEST_FEASIBLE_K_CONTRACT_ID}"
        ));
    }
    if !consumed.contains(BASELINE_NO_REGRESSION_CONTRACT_ID) {
        contract_errors.push(format!(
            "consumed_contracts must include {BASELINE_NO_REGRESSION_CONTRACT_ID}"
        ));
    }

    let mut baseline_metric_ids = BTreeSet::<String>::new();
    let mut baseline_metric_map = BTreeMap::<String, f64>::new();
    for metric in &dossier.baseline_metrics {
        if metric.metric_id.trim().is_empty() {
            completeness_errors.push("baseline_metrics has entry with empty metric_id".to_owned());
            continue;
        }
        if metric.source_artifact.trim().is_empty() {
            completeness_errors.push(format!(
                "baseline_metrics {} missing source_artifact",
                metric.metric_id
            ));
        }
        if !baseline_metric_ids.insert(metric.metric_id.clone()) {
            completeness_errors.push(format!(
                "baseline_metrics has duplicate metric_id {}",
                metric.metric_id
            ));
        }
        baseline_metric_map.insert(metric.metric_id.clone(), metric.value);
    }

    let mut disconfirmation_by_metric = BTreeMap::<String, Vec<&DisconfirmationSignal>>::new();
    for signal in &dossier.disconfirmation_signals {
        if signal.signal_id.trim().is_empty() {
            completeness_errors
                .push("disconfirmation_signals has entry with empty signal_id".to_owned());
        }
        if signal.metric_id.trim().is_empty() {
            completeness_errors
                .push("disconfirmation_signals has entry with empty metric_id".to_owned());
            continue;
        }
        if signal.rationale.trim().is_empty() {
            completeness_errors.push(format!(
                "disconfirmation signal {} missing rationale",
                signal.signal_id
            ));
        }
        disconfirmation_by_metric
            .entry(signal.metric_id.clone())
            .or_default()
            .push(signal);
    }

    for delta in &dossier.expected_metric_deltas {
        if delta.metric_id.trim().is_empty() {
            completeness_errors
                .push("expected_metric_deltas has entry with empty metric_id".to_owned());
            continue;
        }
        if delta.rationale.trim().is_empty() {
            completeness_errors.push(format!(
                "expected_metric_delta {} missing rationale",
                delta.metric_id
            ));
        }
        if !baseline_metric_map.contains_key(&delta.metric_id) {
            contract_errors.push(format!(
                "expected metric {} missing from baseline_metrics",
                delta.metric_id
            ));
        }
        if !has_directionally_valid_threshold(delta) {
            contract_errors.push(format!(
                "expected metric {} has contradictory or missing directional threshold",
                delta.metric_id
            ));
        }
        let Some(signals) = disconfirmation_by_metric.get(&delta.metric_id) else {
            disconfirmation_errors.push(format!(
                "expected metric {} has no disconfirmation signal",
                delta.metric_id
            ));
            continue;
        };
        if !signals_match_direction(delta, signals) {
            disconfirmation_errors.push(format!(
                "expected metric {} has disconfirmation signals that do not match expected direction",
                delta.metric_id
            ));
        }
    }

    let mut errors = Vec::<String>::new();
    errors.extend(
        completeness_errors
            .iter()
            .map(|error| format!("completeness: {error}")),
    );
    errors.extend(
        contract_errors
            .iter()
            .map(|error| format!("contract: {error}")),
    );
    errors.extend(
        disconfirmation_errors
            .iter()
            .map(|error| format!("disconfirmation: {error}")),
    );

    DossierValidationSummary {
        completeness_passed: completeness_errors.is_empty(),
        contract_alignment_passed: contract_errors.is_empty(),
        disconfirmation_passed: disconfirmation_errors.is_empty(),
        errors,
    }
}

fn has_directionally_valid_threshold(delta: &ExpectedMetricDelta) -> bool {
    match delta.direction {
        DeltaDirection::Increase => match (delta.minimum_delta, delta.maximum_delta) {
            (Some(minimum), None) => minimum > 0.0_f64,
            _ => false,
        },
        DeltaDirection::Decrease => match (delta.minimum_delta, delta.maximum_delta) {
            (None, Some(maximum)) => maximum < 0.0_f64,
            _ => false,
        },
        DeltaDirection::NotLower => match (delta.minimum_delta, delta.maximum_delta) {
            (Some(minimum), None) => minimum >= 0.0_f64,
            _ => false,
        },
        DeltaDirection::NotHigher => match (delta.minimum_delta, delta.maximum_delta) {
            (None, Some(maximum)) => maximum <= 0.0_f64,
            _ => false,
        },
    }
}

fn signals_match_direction(
    delta: &ExpectedMetricDelta,
    signals: &[&DisconfirmationSignal],
) -> bool {
    match delta.direction {
        DeltaDirection::Increase | DeltaDirection::NotLower => signals.iter().any(|signal| {
            matches!(
                signal.comparator,
                Comparator::LessThan | Comparator::LessOrEqual
            )
        }),
        DeltaDirection::Decrease | DeltaDirection::NotHigher => signals.iter().any(|signal| {
            matches!(
                signal.comparator,
                Comparator::GreaterThan | Comparator::GreaterOrEqual
            )
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        validate_evidence_dossier, BaselineArtifacts, BaselineMetric, Comparator, DeltaDirection,
        DisconfirmationSignal, EvidenceDossier, ExpectedMetricDelta, Hypothesis,
    };

    fn valid_dossier() -> EvidenceDossier {
        EvidenceDossier {
            contract_id: "C-INVESTIGATIVE-PROOF-FIRST".to_owned(),
            schema_version: 1,
            proposal_id: "proposal-rank-pass-001".to_owned(),
            proposal_title: "Improve rerank scoring for known target ordering".to_owned(),
            proposal_owner: "benchmark-team".to_owned(),
            created_at_utc: "2026-03-01T00:00:00Z".to_owned(),
            consumed_contracts: vec![
                "C-BEST-FEASIBLE-K-RULE".to_owned(),
                "C-BASELINE-NO-REGRESSION".to_owned(),
            ],
            baseline_artifacts: BaselineArtifacts {
                known_baseline_path: "benchmark/known_redaction_baseline.json".to_owned(),
                deterministic_gate_path: "benchmark/determinism_bundle2_probe.json".to_owned(),
                synthetic_gate_path: "benchmark/synthetic_overfitting_evaluation.json".to_owned(),
            },
            baseline_metrics: vec![
                BaselineMetric {
                    metric_id: "known.recall_at_1".to_owned(),
                    value: 0.10_f64,
                    source_artifact: "benchmark/guess_accuracy.json".to_owned(),
                },
                BaselineMetric {
                    metric_id: "known.best_feasible_k".to_owned(),
                    value: 10.0_f64,
                    source_artifact: "benchmark/guess_accuracy.json".to_owned(),
                },
            ],
            hypothesis: Hypothesis {
                mechanism: "Increase contextual rank signal weight for precise anchors".to_owned(),
                expected_effect: "Raise top-ranked placement for known targets".to_owned(),
            },
            expected_metric_deltas: vec![
                ExpectedMetricDelta {
                    metric_id: "known.recall_at_1".to_owned(),
                    direction: DeltaDirection::Increase,
                    minimum_delta: Some(0.05_f64),
                    maximum_delta: None,
                    rationale: "Primary objective is top-1 movement".to_owned(),
                },
                ExpectedMetricDelta {
                    metric_id: "known.best_feasible_k".to_owned(),
                    direction: DeltaDirection::Decrease,
                    minimum_delta: None,
                    maximum_delta: Some(-1.0_f64),
                    rationale: "Fallback objective is lower feasible K".to_owned(),
                },
            ],
            disconfirmation_signals: vec![
                DisconfirmationSignal {
                    signal_id: "known_recall_not_improved".to_owned(),
                    metric_id: "known.recall_at_1".to_owned(),
                    comparator: Comparator::LessOrEqual,
                    threshold: 0.0_f64,
                    rationale: "Reject if recall delta is non-positive".to_owned(),
                },
                DisconfirmationSignal {
                    signal_id: "best_k_not_reduced".to_owned(),
                    metric_id: "known.best_feasible_k".to_owned(),
                    comparator: Comparator::GreaterOrEqual,
                    threshold: 0.0_f64,
                    rationale: "Reject if best feasible K does not decrease".to_owned(),
                },
            ],
        }
    }

    #[test]
    fn dossier_validation_accepts_complete_contract_aligned_input() {
        let dossier = valid_dossier();
        let summary = validate_evidence_dossier(&dossier);
        assert!(summary.completeness_passed);
        assert!(summary.contract_alignment_passed);
        assert!(summary.disconfirmation_passed);
        assert!(summary.errors.is_empty());
    }

    #[test]
    fn dossier_validation_rejects_missing_disconfirmation_signal() {
        let mut dossier = valid_dossier();
        dossier.disconfirmation_signals.clear();
        let summary = validate_evidence_dossier(&dossier);
        assert!(!summary.completeness_passed);
        assert!(!summary.disconfirmation_passed);
        assert!(!summary.errors.is_empty());
    }

    #[test]
    fn dossier_validation_rejects_contract_and_direction_mismatch() {
        let mut dossier = valid_dossier();
        dossier.contract_id = "wrong".to_owned();
        dossier.expected_metric_deltas[0].minimum_delta = Some(0.0_f64);
        let summary = validate_evidence_dossier(&dossier);
        assert!(!summary.contract_alignment_passed);
        assert!(!summary.errors.is_empty());
    }
}
