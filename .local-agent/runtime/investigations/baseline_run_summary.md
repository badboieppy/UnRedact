# Baseline Run Summary
Date: 2026-03-02
Purpose: user-requested full benchmark baseline snapshot (local-only outputs).

## Commands Executed
- `cargo run --bin guess_accuracy_benchmark --release -- --out .local-agent/runtime/investigations/baseline_guess_accuracy.json --baseline-out .local-agent/runtime/investigations/baseline_guess_accuracy.baseline.json`
- `cargo run --bin synthetic_overfitting_benchmark --release -- --out .local-agent/runtime/investigations/baseline_synthetic_overfitting.json`
- `cargo run --bin visual_score_impact_benchmark --release -- --out .local-agent/runtime/investigations/baseline_visual_score_impact.json`
- `cargo run --bin evidence_first_change_gate --release -- --out .local-agent/runtime/investigations/baseline_evidence_gate_decision.json`
- `cargo test`

## Result Snapshot
### guess_accuracy_benchmark
- overall recall@1: 0.090909
- overall recall@5: 0.181818
- overall recall@20: 0.454545
- overall mrr: 0.157513
- overall best_feasible_k: 133
- determinism_gate: enforced=true, passed=true, mismatch_count=0

### synthetic_overfitting_benchmark
- run_completeness_gate: passed=true (expected_seed_runs=11, observed_seed_runs=11)
- fixed_seed_gate: passed=true (mismatch_count=0)
- fixed_seed aggregate: evaluated_items=96, found_items=6
- exploratory summary: evaluated_items=160, found_items=14, recall@5=0.018750, recall@20=0.050000, mrr=0.007975

### visual_score_impact_benchmark
- no_visual overall: evaluated_items=100, found_items=15, recall@20=0.040000, mrr=0.004152
- visual overall: evaluated_items=100, found_items=15, recall@20=0.040000, mrr=0.004152
- pairwise: better=0, worse=0, tie=100, mean_rank_delta=0

### evidence_first_change_gate
- approved=true
- completeness_passed=true
- contract_alignment_passed=true
- disconfirmation_passed=true
- error_count=0

### cargo test
- Pass status: all test targets passed.
- Observed counts in run output: 76 passed, 0 failed, 0 ignored.
