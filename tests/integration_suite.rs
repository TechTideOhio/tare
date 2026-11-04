use std::fs;
use std::path::PathBuf;
use tare::ingest::{load_task_runs, RequestMetrics, TaskRunRecord};
use tare::ledger::TareLedger;
use tare::lockfile::TareLockfile;
use tare::metrics::profile_dataset;
use tare::pareto::{compare_runs, TareVerdict};
use tare::repo_audit::audit_repository;
use tare::spec::TareSpec;

fn fixture_spec() -> TareSpec {
    let toml_str = r#"
[suite]
name = "test-suite"
version = "0.1.0"
description = "Test suite for tare metrology"

[budgets]
max_cpst_usd = 0.0800
max_cpc_usd = 0.0400
max_p50_ms = 1200.0
max_p95_ms = 2500.0
min_cache_hit_rate = 0.25
max_failure_tax_multiplier = 2.0

[gate]
min_quality_floor = 0.80
max_quality_drop = 0.03
tost_equivalence_margin = 0.02
mcnemar_alpha = 0.05
efficiency_win_min_cpst_cut_pct = 20.0
max_cpst_increase_pct = 10.0
max_p95_increase_pct = 15.0

[pricing.default]
input_per_million = 3.00
cache_read_per_million = 0.30
cache_write_per_million = 3.75
output_per_million = 15.00
reasoning_per_million = 15.00

[category.retrieval]
baseline_pass_rate = 0.90
floor_pass_rate = 0.85
max_drop = 0.03
description = "Vector retrieval and RLS tenant isolation"

[category.tool_use]
baseline_pass_rate = 0.90
floor_pass_rate = 0.85
max_drop = 0.03
description = "Tool calling and schema validation"
"#;
    toml::from_str(toml_str).expect("valid toml")
}

fn generate_paired_tasks(
    count: usize,
    base_pass_pct: f64,
    cand_pass_pct: f64,
    base_cost_micros: u64,
    cand_cost_micros: u64,
    base_dur_ms: f64,
    cand_dur_ms: f64,
) -> (Vec<TaskRunRecord>, Vec<TaskRunRecord>) {
    let mut base = Vec::with_capacity(count);
    let mut cand = Vec::with_capacity(count);

    let base_pass_count = ((count as f64) * base_pass_pct).round() as usize;
    let cand_pass_count = ((count as f64) * cand_pass_pct).round() as usize;

    for i in 0..count {
        let task_id = format!("task_{i:04}");
        let cat = if i % 2 == 0 { "retrieval" } else { "tool_use" };

        let b_passed = i < base_pass_count;
        let c_passed = i < cand_pass_count;

        base.push(TaskRunRecord {
            task_id: task_id.clone(),
            category: cat.to_string(),
            passed: b_passed,
            model: "default".to_string(),
            requests: vec![RequestMetrics {
                input_tokens: 1000,
                cached_input_tokens: 500,
                cache_write_tokens: 0,
                output_tokens: 200,
                reasoning_tokens: 50,
                duration_ms: base_dur_ms,
                ttft_ms: Some(base_dur_ms * 0.3),
                cost_usd_micros: Some(base_cost_micros),
            }],
            wall_clock_ms: Some(base_dur_ms),
            retries: 0,
            metadata: std::collections::BTreeMap::new(),
        });

        cand.push(TaskRunRecord {
            task_id,
            category: cat.to_string(),
            passed: c_passed,
            model: "default".to_string(),
            requests: vec![RequestMetrics {
                input_tokens: 600,
                cached_input_tokens: 600,
                cache_write_tokens: 0,
                output_tokens: 150,
                reasoning_tokens: 20,
                duration_ms: cand_dur_ms,
                ttft_ms: Some(cand_dur_ms * 0.3),
                cost_usd_micros: Some(cand_cost_micros),
            }],
            wall_clock_ms: Some(cand_dur_ms),
            retries: 0,
            metadata: std::collections::BTreeMap::new(),
        });
    }

    (base, cand)
}

#[test]
fn test_fixture_loading_and_profiling() {
    let spec = fixture_spec();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let baseline_path = root.join("packs/fixtures/baseline_runs.jsonl");

    let tasks = load_task_runs(&baseline_path).expect("loaded baseline runs");
    assert_eq!(tasks.len(), 50);

    let profile = profile_dataset(&spec, &tasks);
    assert_eq!(profile.total_tasks, 50);
    assert_eq!(profile.passed_tasks, 46);
    assert!(profile.costs.cpst_usd.is_some());
}

#[test]
fn test_quality_flat_40pct_cost_cut_ships() {
    let spec = fixture_spec();
    // 100 tasks, both 90% pass rate (flat quality)
    // Base cost: $0.0500 per task (50,000 micros) -> CPST = $0.0555
    // Cand cost: $0.0290 per task (29,000 micros) -> CPST = $0.0322 (-42% cut!)
    // Base p95: 1800ms, Cand p95: 1100ms (-39% cut!)
    let (base, cand) = generate_paired_tasks(100, 0.90, 0.90, 50_000, 29_000, 1500.0, 950.0);

    let report = compare_runs(&spec, &base, &cand).expect("comparison ok");

    assert_eq!(report.verdict, TareVerdict::QualityFlatEfficiencyWin);
    assert!(report.verdict.is_ship_approved());
    assert!(report.cpst_delta_pct <= -40.0);
}

#[test]
fn test_vanity_call_cost_trap_is_blocked() {
    let spec = fixture_spec();
    // Base: 90% pass, cost $0.0400 (40,000 micros) -> CPST = $0.0444
    // Cand: 30% pass, cost $0.0200 (20,000 micros per call [-50% CPC!])
    // But because pass rate collapsed, CPST = 20,000 / 0.30 = $0.0667 (+50% increase!)
    let (base, cand) = generate_paired_tasks(100, 0.90, 0.30, 40_000, 20_000, 1200.0, 3200.0);

    let report = compare_runs(&spec, &base, &cand).expect("comparison ok");

    // Must be blocked! Either VanityCallCostTrapBlocked or QualityRegressionBlocked
    assert!(!report.verdict.is_ship_approved());
}

#[test]
fn test_latency_sla_breach_is_blocked() {
    let spec = fixture_spec();
    // Base: 90% pass, dur 1200ms
    // Cand: 91% pass, dur 4500ms (exceeds max_p95_ms of 2500ms!)
    let (base, cand) = generate_paired_tasks(100, 0.90, 0.91, 40_000, 41_000, 1200.0, 4500.0);

    let report = compare_runs(&spec, &base, &cand).expect("comparison ok");

    assert_eq!(report.verdict, TareVerdict::LatencySlaBreachBlocked);
    assert!(!report.verdict.is_ship_approved());
}

#[test]
fn test_ledger_merkle_provenance_tamper_detection() {
    let spec = fixture_spec();
    let (tasks, _) = generate_paired_tasks(10, 0.90, 0.90, 30_000, 30_000, 1000.0, 1000.0);

    let temp_dir = std::env::temp_dir();
    let ledger_path = temp_dir.join("tare_test_tamper.ledger.jsonl");

    let ledger = TareLedger::create_from_tasks(&tasks, &spec);
    ledger.save_to_file(&ledger_path).expect("saved");

    // Verification passes on untouched ledger
    let loaded = TareLedger::load_and_verify(&ledger_path).expect("verified");
    assert_eq!(loaded.total_entries, 10);

    // Tamper with one entry
    let content = fs::read_to_string(&ledger_path).expect("read");
    let tampered = content.replacen("\"cost_usd_micros\":30000", "\"cost_usd_micros\":99999", 1);
    fs::write(&ledger_path, tampered).expect("write tampered");

    // Verification MUST fail
    let tamper_res = TareLedger::load_and_verify(&ledger_path);
    assert!(
        tamper_res.is_err(),
        "tampered ledger must fail verification"
    );

    let _ = fs::remove_file(ledger_path);
}

#[test]
fn test_lockfile_creation_and_drift_verification() {
    let temp_dir = std::env::temp_dir();
    let spec_path = temp_dir.join("tare_test_spec.toml");
    let lock_path = temp_dir.join("tare_test_spec.lock.json");

    let starter = r#"
[suite]
name = "lockfile-test"
version = "0.1.0"
description = "test"

[category.retrieval]
baseline_pass_rate = 0.92
description = "retrieval"
"#;
    fs::write(&spec_path, starter).expect("write spec");

    let spec = TareSpec::from_file(&spec_path).expect("parsed spec");
    let lockfile = TareLockfile::create(&spec_path, &spec).expect("lock created");
    lockfile.save_to_file(&lock_path).expect("saved lock");

    // Verification passes
    lockfile
        .verify(&spec_path, &spec)
        .expect("lockfile verified");

    // Mutate spec to cause drift
    let mutated = starter.replace("0.92", "0.85");
    fs::write(&spec_path, mutated).expect("write mutated");
    let mutated_spec = TareSpec::from_file(&spec_path).expect("parsed mutated");

    let verify_res = lockfile.verify(&spec_path, &mutated_spec);
    assert!(
        verify_res.is_err(),
        "drifted spec must fail lockfile verification"
    );

    let _ = fs::remove_file(spec_path);
    let _ = fs::remove_file(lock_path);
}

#[test]
fn test_softfactory_repo_audit_passes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let report = audit_repository(&root);
    assert!(
        report.passed,
        "Repo audit failed: {:?}",
        report
            .gates
            .iter()
            .filter(|g| !g.passed)
            .map(|g| (&g.name, &g.details))
            .collect::<Vec<_>>()
    );
    assert_eq!(report.passed_gates, report.total_gates);
    assert_eq!(report.total_gates, 8);
}
