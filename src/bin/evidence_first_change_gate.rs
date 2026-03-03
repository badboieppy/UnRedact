use clap::{Parser, ValueEnum};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use unredact::benchmarks::types::evidence_dossier_contract::{
    validate_evidence_dossier, DossierValidationSummary, EvidenceDossier,
};

const EVIDENCE_GATE_POLICY: &str = "evidence_first_change_gate_v1";
const INTENT_THRESHOLD_POLICY: &str = "intent_aware_threshold_gate_v1";
const NO_RETRY_REGRESSION_POLICY: &str = "no_retry_investigate_then_rollback";

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
enum BundleIntent {
    Neutral,
    Improve,
}

#[derive(Debug, Parser)]
#[command(
    name = "evidence_first_change_gate",
    about = "Approve or reject a candidate change proposal based on a machine-checkable evidence dossier."
)]
struct CliOptions {
    #[arg(
        long,
        default_value = "src/benchmarks/contracts/evidence_dossier_example_valid.json",
        help = "Path to the proposal evidence dossier JSON."
    )]
    dossier: PathBuf,
    #[arg(
        long,
        default_value = "benchmark/evidence_first_change_gate_decision.json",
        help = "Output path for gate decision JSON."
    )]
    out: PathBuf,
    #[arg(long, value_enum, default_value_t = BundleIntent::Neutral)]
    intent: BundleIntent,
    #[arg(long, default_value_t = 0.01_f64)]
    margin_ratio: f64,
    #[arg(long, default_value = "benchmark/guess_accuracy.json")]
    guess_current: PathBuf,
    #[arg(long, default_value = "benchmark/guess_accuracy.baseline.json")]
    guess_baseline: PathBuf,
    #[arg(
        long,
        default_value = "benchmark/synthetic_overfitting_evaluation.json"
    )]
    synthetic_current: PathBuf,
    #[arg(long, default_value = "benchmark/visual_score_impact.json")]
    visual_current: PathBuf,
    #[arg(long, default_value = "benchmark/regression_response.json")]
    regression_out: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct EvidenceGateDecision {
    policy: String,
    dossier_path: String,
    dossier_sha256: Option<String>,
    contract_id: Option<String>,
    schema_version: Option<usize>,
    proposal_id: Option<String>,
    approved: bool,
    completeness_passed: bool,
    contract_alignment_passed: bool,
    disconfirmation_passed: bool,
    benchmark_gate_passed: bool,
    benchmark_gate: BenchmarkGateDecision,
    regression_action: RegressionActionDecision,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct BenchmarkGateDecision {
    policy: String,
    intent: BundleIntent,
    margin_ratio: f64,
    passed: bool,
    intent_gate: IntentThresholdGate,
    synthetic_gate: SyntheticGateDecision,
    visual_gate: VisualGateDecision,
}

#[derive(Debug, Clone, Serialize)]
struct IntentThresholdGate {
    current_path: String,
    baseline_path: String,
    improvement_required: bool,
    improvement_observed: bool,
    passed: bool,
    metrics: Vec<MetricDecision>,
    failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct MetricDecision {
    metric: String,
    direction: String,
    baseline: Option<f64>,
    observed: Option<f64>,
    delta: Option<f64>,
    allowed_negative_delta: Option<f64>,
    passed: bool,
}

#[derive(Debug, Clone, Serialize)]
struct SyntheticGateDecision {
    current_path: String,
    run_completeness_passed: bool,
    fixed_seed_passed: bool,
    multi_seed_panel_present: bool,
    multi_seed_panel_passed: bool,
    multi_seed_panel_seed_count: Option<usize>,
    multi_seed_panel_required_min_seed_count: Option<usize>,
    passed: bool,
    failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct VisualGateDecision {
    current_path: String,
    recall_at_20_delta_visual_minus_no_visual: f64,
    mrr_delta_visual_minus_no_visual: f64,
    allowed_negative_delta_recall_at_20: f64,
    allowed_negative_delta_mrr: f64,
    passed: bool,
    failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RegressionActionDecision {
    policy: String,
    triggered: bool,
    no_retry: bool,
    rollback_required: bool,
    rollback_executed: bool,
    artifact_path: Option<String>,
    reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RegressionActionArtifact {
    policy: String,
    triggered: bool,
    no_retry: bool,
    rollback_required: bool,
    rollback_executed: bool,
    intent: BundleIntent,
    reasons: Vec<String>,
    guess_current: String,
    guess_baseline: String,
    synthetic_current: String,
    visual_current: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GuessAccuracyReport {
    overall: GuessBenchmarkSummary,
}

#[derive(Debug, Clone, Deserialize)]
struct GuessBenchmarkSummary {
    recall_at_1: f64,
    recall_at_5: f64,
    recall_at_20: f64,
    mrr: f64,
    mean_rank_found: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct SyntheticBenchmarkReport {
    run_completeness_gate: PassedGate,
    fixed_seed_gate: PassedGate,
    #[serde(default)]
    multi_seed_panel: Option<SyntheticMultiSeedPanel>,
}

#[derive(Debug, Clone, Deserialize)]
struct PassedGate {
    passed: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct SyntheticMultiSeedPanel {
    passed: bool,
    seed_count: usize,
    required_min_seed_count: usize,
    #[serde(default)]
    failures: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct VisualScoreImpactReport {
    no_visual_overall: VisualBenchmarkSummary,
    visual_overall: VisualBenchmarkSummary,
}

#[derive(Debug, Clone, Deserialize)]
struct VisualBenchmarkSummary {
    recall_at_20: f64,
    mrr: f64,
}

fn main() {
    let options = CliOptions::parse();
    if let Err(error) = run(options) {
        eprintln!("evidence first change gate failed: {error}");
        std::process::exit(1);
    }
}

fn run(options: CliOptions) -> Result<(), String> {
    let bytes = std::fs::read(&options.dossier).map_err(|error| {
        format!(
            "failed to read dossier {}: {error}",
            options.dossier.display()
        )
    })?;
    let dossier_hash = sha256_hex(&bytes);

    let mut decision = match serde_json::from_slice::<EvidenceDossier>(&bytes) {
        Ok(dossier) => decision_from_dossier(
            &options.dossier,
            Some(dossier_hash),
            &dossier,
            validate_evidence_dossier(&dossier),
        ),
        Err(error) => EvidenceGateDecision {
            policy: EVIDENCE_GATE_POLICY.to_owned(),
            dossier_path: options.dossier.display().to_string(),
            dossier_sha256: Some(dossier_hash),
            contract_id: None,
            schema_version: None,
            proposal_id: None,
            approved: false,
            completeness_passed: false,
            contract_alignment_passed: false,
            disconfirmation_passed: false,
            benchmark_gate_passed: false,
            benchmark_gate: empty_benchmark_gate(options.intent, options.margin_ratio),
            regression_action: default_regression_action(),
            errors: vec![format!("parse: failed to parse dossier json: {error}")],
        },
    };

    let benchmark_gate = match evaluate_benchmark_gate(&options) {
        Ok(value) => value,
        Err(error) => {
            let mut gate = empty_benchmark_gate(options.intent, options.margin_ratio);
            gate.passed = false;
            gate.intent_gate.passed = false;
            gate.intent_gate
                .failures
                .push(format!("benchmark_input_error: {error}"));
            gate
        }
    };
    decision.benchmark_gate_passed = benchmark_gate.passed;
    decision.benchmark_gate = benchmark_gate.clone();
    if !benchmark_gate.passed {
        let mut reasons = Vec::<String>::new();
        reasons.extend(benchmark_gate.intent_gate.failures.iter().cloned());
        reasons.extend(benchmark_gate.synthetic_gate.failures.iter().cloned());
        reasons.extend(benchmark_gate.visual_gate.failures.iter().cloned());
        if reasons.is_empty() {
            reasons.push("benchmark gate failed without reported failure details".to_owned());
        }
        decision.approved = false;
        decision.errors.extend(
            reasons
                .iter()
                .map(|reason| format!("benchmark_gate: {reason}"))
                .collect::<Vec<_>>(),
        );
        let artifact = RegressionActionArtifact {
            policy: NO_RETRY_REGRESSION_POLICY.to_owned(),
            triggered: true,
            no_retry: true,
            rollback_required: true,
            rollback_executed: false,
            intent: options.intent,
            reasons: reasons.clone(),
            guess_current: options.guess_current.display().to_string(),
            guess_baseline: options.guess_baseline.display().to_string(),
            synthetic_current: options.synthetic_current.display().to_string(),
            visual_current: options.visual_current.display().to_string(),
        };
        write_json(&options.regression_out, &artifact)?;
        decision.regression_action = RegressionActionDecision {
            policy: NO_RETRY_REGRESSION_POLICY.to_owned(),
            triggered: true,
            no_retry: true,
            rollback_required: true,
            rollback_executed: false,
            artifact_path: Some(options.regression_out.display().to_string()),
            reasons,
        };
    }

    print_decision(&decision);
    write_json(&options.out, &decision)?;
    println!("wrote {}", options.out.display());
    if !decision.approved {
        return Err(format!(
            "proposal rejected by evidence gate with {} errors",
            decision.errors.len()
        ));
    }
    Ok(())
}

fn evaluate_benchmark_gate(options: &CliOptions) -> Result<BenchmarkGateDecision, String> {
    let intent_gate = evaluate_intent_threshold_gate(
        options.intent,
        options.margin_ratio,
        &options.guess_current,
        &options.guess_baseline,
    )?;
    let synthetic_gate = evaluate_synthetic_gate(&options.synthetic_current)?;
    let visual_gate = evaluate_visual_gate(options.margin_ratio, &options.visual_current)?;
    Ok(BenchmarkGateDecision {
        policy: INTENT_THRESHOLD_POLICY.to_owned(),
        intent: options.intent,
        margin_ratio: options.margin_ratio,
        passed: intent_gate.passed && synthetic_gate.passed && visual_gate.passed,
        intent_gate,
        synthetic_gate,
        visual_gate,
    })
}

fn evaluate_intent_threshold_gate(
    intent: BundleIntent,
    margin_ratio: f64,
    current_path: &Path,
    baseline_path: &Path,
) -> Result<IntentThresholdGate, String> {
    let current = read_json::<GuessAccuracyReport>(current_path)?;
    let baseline = read_json::<GuessAccuracyReport>(baseline_path)?;
    let mut metrics = Vec::<MetricDecision>::new();
    let mut failures = Vec::<String>::new();
    let mut improvement_observed = false;

    let eval_higher = |metric: &str,
                       baseline_value: f64,
                       observed_value: f64,
                       metrics: &mut Vec<MetricDecision>,
                       failures: &mut Vec<String>,
                       improvement_observed: &mut bool| {
        let allowed_drop = baseline_value.abs() * margin_ratio;
        let delta = observed_value - baseline_value;
        let passed = delta >= -allowed_drop;
        if !passed {
            failures.push(format!(
                "{metric} regression delta={delta:.6} allowed_negative_delta={:.6}",
                allowed_drop
            ));
        }
        if delta > allowed_drop {
            *improvement_observed = true;
        }
        metrics.push(MetricDecision {
            metric: metric.to_owned(),
            direction: "higher_is_better".to_owned(),
            baseline: Some(baseline_value),
            observed: Some(observed_value),
            delta: Some(delta),
            allowed_negative_delta: Some(-allowed_drop),
            passed,
        });
    };

    eval_higher(
        "overall.recall_at_1",
        baseline.overall.recall_at_1,
        current.overall.recall_at_1,
        &mut metrics,
        &mut failures,
        &mut improvement_observed,
    );
    eval_higher(
        "overall.recall_at_5",
        baseline.overall.recall_at_5,
        current.overall.recall_at_5,
        &mut metrics,
        &mut failures,
        &mut improvement_observed,
    );
    eval_higher(
        "overall.recall_at_20",
        baseline.overall.recall_at_20,
        current.overall.recall_at_20,
        &mut metrics,
        &mut failures,
        &mut improvement_observed,
    );
    eval_higher(
        "overall.mrr",
        baseline.overall.mrr,
        current.overall.mrr,
        &mut metrics,
        &mut failures,
        &mut improvement_observed,
    );

    let mean_rank_metric = match (
        baseline.overall.mean_rank_found,
        current.overall.mean_rank_found,
    ) {
        (Some(baseline_value), Some(observed_value)) => {
            let allowed_increase = baseline_value.abs() * margin_ratio;
            let delta = baseline_value - observed_value;
            let passed = delta >= -allowed_increase;
            if !passed {
                failures.push(format!(
                    "overall.mean_rank_found regression delta={delta:.6} allowed_negative_delta={:.6}",
                    allowed_increase
                ));
            }
            if delta > allowed_increase {
                improvement_observed = true;
            }
            MetricDecision {
                metric: "overall.mean_rank_found".to_owned(),
                direction: "lower_is_better".to_owned(),
                baseline: Some(baseline_value),
                observed: Some(observed_value),
                delta: Some(delta),
                allowed_negative_delta: Some(-allowed_increase),
                passed,
            }
        }
        (None, None) => MetricDecision {
            metric: "overall.mean_rank_found".to_owned(),
            direction: "lower_is_better".to_owned(),
            baseline: None,
            observed: None,
            delta: None,
            allowed_negative_delta: None,
            passed: true,
        },
        (Some(_), None) => {
            failures.push(
                "overall.mean_rank_found missing in current report while present in baseline"
                    .to_owned(),
            );
            MetricDecision {
                metric: "overall.mean_rank_found".to_owned(),
                direction: "lower_is_better".to_owned(),
                baseline: baseline.overall.mean_rank_found,
                observed: current.overall.mean_rank_found,
                delta: None,
                allowed_negative_delta: None,
                passed: false,
            }
        }
        (None, Some(_)) => MetricDecision {
            metric: "overall.mean_rank_found".to_owned(),
            direction: "lower_is_better".to_owned(),
            baseline: baseline.overall.mean_rank_found,
            observed: current.overall.mean_rank_found,
            delta: None,
            allowed_negative_delta: None,
            passed: true,
        },
    };
    metrics.push(mean_rank_metric);

    let improvement_required = intent == BundleIntent::Improve;
    if improvement_required && !improvement_observed {
        failures.push(
            "bundle intent is improve but no core accuracy metric improved beyond margin band"
                .to_owned(),
        );
    }

    Ok(IntentThresholdGate {
        current_path: current_path.display().to_string(),
        baseline_path: baseline_path.display().to_string(),
        improvement_required,
        improvement_observed,
        passed: failures.is_empty(),
        metrics,
        failures,
    })
}

fn evaluate_synthetic_gate(current_path: &Path) -> Result<SyntheticGateDecision, String> {
    let current = read_json::<SyntheticBenchmarkReport>(current_path)?;
    let mut failures = Vec::<String>::new();
    let run_completeness_passed = current.run_completeness_gate.passed;
    let fixed_seed_passed = current.fixed_seed_gate.passed;
    if !run_completeness_passed {
        failures.push("synthetic run_completeness_gate failed".to_owned());
    }
    if !fixed_seed_passed {
        failures.push("synthetic fixed_seed_gate failed".to_owned());
    }
    let multi_seed_panel_present = current.multi_seed_panel.is_some();
    let (multi_seed_panel_passed, seed_count, required_seed_count) =
        if let Some(panel) = current.multi_seed_panel.as_ref() {
            failures.extend(panel.failures.iter().cloned());
            if panel.seed_count < panel.required_min_seed_count {
                failures.push(format!(
                    "synthetic multi_seed_panel seed_count below minimum observed={} required={}",
                    panel.seed_count, panel.required_min_seed_count
                ));
            }
            (
                panel.passed,
                Some(panel.seed_count),
                Some(panel.required_min_seed_count),
            )
        } else {
            failures.push("synthetic multi_seed_panel section missing".to_owned());
            (false, None, None)
        };
    if !multi_seed_panel_passed {
        failures.push("synthetic multi_seed_panel failed".to_owned());
    }
    Ok(SyntheticGateDecision {
        current_path: current_path.display().to_string(),
        run_completeness_passed,
        fixed_seed_passed,
        multi_seed_panel_present,
        multi_seed_panel_passed,
        multi_seed_panel_seed_count: seed_count,
        multi_seed_panel_required_min_seed_count: required_seed_count,
        passed: failures.is_empty(),
        failures,
    })
}

fn evaluate_visual_gate(
    margin_ratio: f64,
    current_path: &Path,
) -> Result<VisualGateDecision, String> {
    let current = read_json::<VisualScoreImpactReport>(current_path)?;
    let recall_delta = current.visual_overall.recall_at_20 - current.no_visual_overall.recall_at_20;
    let mrr_delta = current.visual_overall.mrr - current.no_visual_overall.mrr;
    let allowed_negative_delta_recall_at_20 =
        current.no_visual_overall.recall_at_20.abs() * margin_ratio;
    let allowed_negative_delta_mrr = current.no_visual_overall.mrr.abs() * margin_ratio;
    let mut failures = Vec::<String>::new();
    if recall_delta < -allowed_negative_delta_recall_at_20 {
        failures.push(format!(
            "visual recall_at_20 regression delta={recall_delta:.6} allowed_negative_delta={allowed_negative_delta_recall_at_20:.6}"
        ));
    }
    if mrr_delta < -allowed_negative_delta_mrr {
        failures.push(format!(
            "visual mrr regression delta={mrr_delta:.6} allowed_negative_delta={allowed_negative_delta_mrr:.6}"
        ));
    }
    Ok(VisualGateDecision {
        current_path: current_path.display().to_string(),
        recall_at_20_delta_visual_minus_no_visual: recall_delta,
        mrr_delta_visual_minus_no_visual: mrr_delta,
        allowed_negative_delta_recall_at_20,
        allowed_negative_delta_mrr,
        passed: failures.is_empty(),
        failures,
    })
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read benchmark json {}: {error}", path.display()))?;
    serde_json::from_slice::<T>(&bytes)
        .map_err(|error| format!("failed to parse benchmark json {}: {error}", path.display()))
}

fn empty_benchmark_gate(intent: BundleIntent, margin_ratio: f64) -> BenchmarkGateDecision {
    BenchmarkGateDecision {
        policy: INTENT_THRESHOLD_POLICY.to_owned(),
        intent,
        margin_ratio,
        passed: true,
        intent_gate: IntentThresholdGate {
            current_path: String::new(),
            baseline_path: String::new(),
            improvement_required: intent == BundleIntent::Improve,
            improvement_observed: intent != BundleIntent::Improve,
            passed: true,
            metrics: Vec::new(),
            failures: Vec::new(),
        },
        synthetic_gate: SyntheticGateDecision {
            current_path: String::new(),
            run_completeness_passed: true,
            fixed_seed_passed: true,
            multi_seed_panel_present: true,
            multi_seed_panel_passed: true,
            multi_seed_panel_seed_count: None,
            multi_seed_panel_required_min_seed_count: None,
            passed: true,
            failures: Vec::new(),
        },
        visual_gate: VisualGateDecision {
            current_path: String::new(),
            recall_at_20_delta_visual_minus_no_visual: 0.0_f64,
            mrr_delta_visual_minus_no_visual: 0.0_f64,
            allowed_negative_delta_recall_at_20: 0.0_f64,
            allowed_negative_delta_mrr: 0.0_f64,
            passed: true,
            failures: Vec::new(),
        },
    }
}

fn default_regression_action() -> RegressionActionDecision {
    RegressionActionDecision {
        policy: NO_RETRY_REGRESSION_POLICY.to_owned(),
        triggered: false,
        no_retry: true,
        rollback_required: false,
        rollback_executed: false,
        artifact_path: None,
        reasons: Vec::new(),
    }
}

fn decision_from_dossier(
    dossier_path: &Path,
    dossier_sha256: Option<String>,
    dossier: &EvidenceDossier,
    validation: DossierValidationSummary,
) -> EvidenceGateDecision {
    EvidenceGateDecision {
        policy: EVIDENCE_GATE_POLICY.to_owned(),
        dossier_path: dossier_path.display().to_string(),
        dossier_sha256,
        contract_id: Some(dossier.contract_id.clone()),
        schema_version: Some(dossier.schema_version),
        proposal_id: Some(dossier.proposal_id.clone()),
        approved: validation.errors.is_empty(),
        completeness_passed: validation.completeness_passed,
        contract_alignment_passed: validation.contract_alignment_passed,
        disconfirmation_passed: validation.disconfirmation_passed,
        benchmark_gate_passed: true,
        benchmark_gate: empty_benchmark_gate(BundleIntent::Neutral, 0.01_f64),
        regression_action: default_regression_action(),
        errors: validation.errors,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create output directory {}: {error}",
                    parent.display()
                )
            })?;
        }
    }
    let encoded = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("failed to encode decision json: {error}"))?;
    std::fs::write(path, encoded)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn print_decision(decision: &EvidenceGateDecision) {
    println!(
        "EVIDENCE_GATE policy={} approved={} completeness_passed={} contract_alignment_passed={} disconfirmation_passed={} benchmark_gate_passed={} error_count={}",
        decision.policy,
        decision.approved,
        decision.completeness_passed,
        decision.contract_alignment_passed,
        decision.disconfirmation_passed,
        decision.benchmark_gate_passed,
        decision.errors.len()
    );
    println!(
        "BENCHMARK_GATE policy={} intent={:?} margin_ratio={} passed={} intent_passed={} synthetic_passed={} visual_passed={}",
        decision.benchmark_gate.policy,
        decision.benchmark_gate.intent,
        decision.benchmark_gate.margin_ratio,
        decision.benchmark_gate.passed,
        decision.benchmark_gate.intent_gate.passed,
        decision.benchmark_gate.synthetic_gate.passed,
        decision.benchmark_gate.visual_gate.passed
    );
    println!("dossier_path={}", decision.dossier_path);
    if let Some(value) = &decision.proposal_id {
        println!("proposal_id={value}");
    }
    if let Some(value) = &decision.dossier_sha256 {
        println!("dossier_sha256={value}");
    }
    if decision.regression_action.triggered {
        println!(
            "REGRESSION_ACTION policy={} no_retry={} rollback_required={} rollback_executed={} artifact_path={}",
            decision.regression_action.policy,
            decision.regression_action.no_retry,
            decision.regression_action.rollback_required,
            decision.regression_action.rollback_executed,
            decision
                .regression_action
                .artifact_path
                .as_deref()
                .unwrap_or("-")
        );
        for reason in &decision.regression_action.reasons {
            println!("REGRESSION_REASON {reason}");
        }
    }
    for error in &decision.errors {
        println!("EVIDENCE_GATE_ERROR {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::{decision_from_dossier, EvidenceGateDecision};
    use std::path::Path;
    use unredact::benchmarks::types::evidence_dossier_contract::{
        validate_evidence_dossier, EvidenceDossier,
    };

    fn parse_fixture(json_text: &str) -> EvidenceDossier {
        serde_json::from_str::<EvidenceDossier>(json_text).expect("fixture should parse")
    }

    fn fixture_decision(json_text: &str) -> EvidenceGateDecision {
        let dossier = parse_fixture(json_text);
        let validation = validate_evidence_dossier(&dossier);
        decision_from_dossier(Path::new("fixture.json"), None, &dossier, validation)
    }

    #[test]
    fn gate_approves_complete_fixture_dossier() {
        let decision = fixture_decision(include_str!(
            "../benchmarks/contracts/evidence_dossier_example_valid.json"
        ));
        assert!(decision.approved);
        assert!(decision.completeness_passed);
        assert!(decision.contract_alignment_passed);
        assert!(decision.disconfirmation_passed);
        assert!(decision.errors.is_empty());
    }

    #[test]
    fn gate_rejects_incomplete_fixture_dossier() {
        let decision = fixture_decision(include_str!(
            "../benchmarks/contracts/evidence_dossier_example_incomplete.json"
        ));
        assert!(!decision.approved);
        assert!(!decision.completeness_passed);
        assert!(!decision.contract_alignment_passed);
        assert!(!decision.disconfirmation_passed);
        assert!(!decision.errors.is_empty());
    }
}
