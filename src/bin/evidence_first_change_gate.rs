use clap::Parser;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use unredact::benchmarks::types::evidence_dossier_contract::{
    validate_evidence_dossier, DossierValidationSummary, EvidenceDossier,
};

const EVIDENCE_GATE_POLICY: &str = "evidence_first_change_gate_v1";

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
    errors: Vec<String>,
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

    let decision = match serde_json::from_slice::<EvidenceDossier>(&bytes) {
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
            errors: vec![format!("parse: failed to parse dossier json: {error}")],
        },
    };

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
        "EVIDENCE_GATE policy={} approved={} completeness_passed={} contract_alignment_passed={} disconfirmation_passed={} error_count={}",
        decision.policy,
        decision.approved,
        decision.completeness_passed,
        decision.contract_alignment_passed,
        decision.disconfirmation_passed,
        decision.errors.len()
    );
    println!("dossier_path={}", decision.dossier_path);
    if let Some(value) = &decision.proposal_id {
        println!("proposal_id={value}");
    }
    if let Some(value) = &decision.dossier_sha256 {
        println!("dossier_sha256={value}");
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
