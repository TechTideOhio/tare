#![forbid(unsafe_code)]

use clap::{Args, Parser, Subcommand};
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use tare::economics::ProductionEconomics;
use tare::envelope::{EnvelopeStatus, HarnessEnvelope};
use tare::ingest::load_task_runs;
use tare::ledger::TareLedger;
use tare::lockfile::TareLockfile;
use tare::metrics::profile_dataset;
use tare::pareto::compare_runs;
use tare::repo_audit::audit_repository;
use tare::report::{
    generate_otel_spans, render_markdown_summary, render_mermaid_diagram,
    render_reference_physics_table, render_terminal_comparison_table,
    render_terminal_profile_table,
};
use tare::spec::TareSpec;

#[derive(Parser, Debug)]
#[command(
    name = "tare",
    about = "TideOS Eval Kit (Tool 14): Cost and latency first-class metrology, tokens per request, Cost Per Successful Task (CPST), and p50/p95 Pareto release gating",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(
        about = "Display the Cost-Per-Successful-Task & Latency Pareto metrology reference table"
    )]
    Table,

    #[command(about = "Initialize a starter tare.toml specification")]
    Init(InitArgs),

    #[command(
        about = "Profile a task execution dataset (tokens/req, CPST, CPC, p50/p95 quantiles)"
    )]
    Profile(ProfileArgs),

    #[command(about = "Paired comparative metrology between baseline and candidate runs")]
    Compare(CompareArgs),

    #[command(about = "Fail-closed release gate for CI/CD with Pareto decision matrix")]
    Gate(GateArgs),

    #[command(about = "Create or verify a sequential SHA-256 Merkle task provenance ledger")]
    Ledger(LedgerArgs),

    #[command(about = "Generate a sha256-framed-v1 cryptographic lockfile")]
    Lock(LockArgs),

    #[command(about = "Verify lockfile cryptographic integrity against specification")]
    Verify(VerifyArgs),

    #[command(
        about = "Audit repository for Soft Factory hygiene, OSS governance, and zero em-dash compliance"
    )]
    Audit(AuditArgs),
}

#[derive(Args, Debug)]
pub struct InitArgs {
    #[arg(short, long, default_value = "tare.toml")]
    pub output: PathBuf,

    #[arg(short, long)]
    pub force: bool,

    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ProfileArgs {
    #[arg(short, long, default_value = "tare.toml")]
    pub spec: PathBuf,

    #[arg(short, long)]
    pub traces: PathBuf,

    #[arg(long)]
    pub ledger: Option<PathBuf>,

    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct CompareArgs {
    #[arg(short, long, default_value = "tare.toml")]
    pub spec: PathBuf,

    #[arg(short, long)]
    pub baseline: PathBuf,

    #[arg(short, long)]
    pub candidate: PathBuf,

    #[arg(long, default_value = "table")]
    pub format: String,

    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct GateArgs {
    #[arg(short, long, default_value = "tare.toml")]
    pub spec: PathBuf,

    #[arg(short, long)]
    pub baseline: PathBuf,

    #[arg(short, long)]
    pub candidate: PathBuf,

    #[arg(long)]
    pub markdown: Option<PathBuf>,

    #[arg(long)]
    pub otel: Option<PathBuf>,

    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct LedgerArgs {
    #[arg(short, long, default_value = "tare.toml")]
    pub spec: PathBuf,

    #[arg(short, long)]
    pub traces: PathBuf,

    #[arg(short, long, default_value = "tare.ledger.jsonl")]
    pub output: PathBuf,

    #[arg(long)]
    pub verify: bool,

    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct LockArgs {
    #[arg(short, long, default_value = "tare.toml")]
    pub spec: PathBuf,

    #[arg(short, long, default_value = "tare.lock.json")]
    pub output: PathBuf,

    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct VerifyArgs {
    #[arg(short, long, default_value = "tare.toml")]
    pub spec: PathBuf,

    #[arg(short, long, default_value = "tare.lock.json")]
    pub lock: PathBuf,

    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct AuditArgs {
    #[arg(default_value = ".")]
    pub root: PathBuf,

    #[arg(long)]
    pub json: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Commands::Table => handle_table(),
        Commands::Init(args) => handle_init(args),
        Commands::Profile(args) => handle_profile(args),
        Commands::Compare(args) => handle_compare(args),
        Commands::Gate(args) => handle_gate(args),
        Commands::Ledger(args) => handle_ledger(args),
        Commands::Lock(args) => handle_lock(args),
        Commands::Verify(args) => handle_verify(args),
        Commands::Audit(args) => handle_audit(args),
    }
}

fn handle_table() -> ExitCode {
    println!("{}", render_reference_physics_table());
    ExitCode::SUCCESS
}

fn handle_init(args: InitArgs) -> ExitCode {
    if args.output.exists() && !args.force {
        eprintln!(
            "Error: file '{}' already exists. Use --force to overwrite.",
            args.output.display()
        );
        return ExitCode::from(2);
    }

    let starter = r#"[suite]
name = "softfactory-cost-latency-evals"
version = "0.1.0"
description = "First-class cost, token, and latency metrology with 3D Pareto release gating"

[budgets]
max_cpst_usd = 0.0800
max_cpc_usd = 0.0400
max_p50_ms = 1200.0
max_p95_ms = 2500.0
min_cache_hit_rate = 0.25
max_failure_tax_multiplier = 2.0

[gate]
min_quality_floor = 0.85
max_quality_drop = 0.02
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

[pricing.claude-3-7-sonnet]
input_per_million = 3.00
cache_read_per_million = 0.30
cache_write_per_million = 3.75
output_per_million = 15.00
reasoning_per_million = 15.00

[pricing.claude-3-5-haiku]
input_per_million = 0.80
cache_read_per_million = 0.08
cache_write_per_million = 1.00
output_per_million = 4.00
reasoning_per_million = 4.00

[category.retrieval]
baseline_pass_rate = 0.96
floor_pass_rate = 0.88
max_drop = 0.03
target_cpst_usd = 0.0450
target_p95_ms = 1800.0
description = "Throughline Studio vector retrieval, chunk grounding, and RLS tenant isolation"

[category.tool_use]
baseline_pass_rate = 0.94
floor_pass_rate = 0.85
max_drop = 0.04
target_cpst_usd = 0.0400
target_p95_ms = 1500.0
description = "Agent MCP tool call syntax, argument schemas, and execution efficiency"

[category.prompt_fidelity]
baseline_pass_rate = 0.92
floor_pass_rate = 0.80
max_drop = 0.05
target_cpst_usd = 0.0350
target_p95_ms = 1200.0
description = "Persona boundary adherence, concise structured generation, and cache prefix alignment"

[economics]
daily_successful_tasks = 50000
developer_hourly_rate_usd = 150.0
latency_seconds_to_dev_cost_ratio = 0.04
"#;

    if let Err(e) = fs::write(&args.output, starter) {
        eprintln!("Failed to write {}: {}", args.output.display(), e);
        return ExitCode::from(2);
    }

    if args.json {
        let env = HarnessEnvelope::success(
            format!("Initialized specification at {}", args.output.display()),
            vec![args.output.display().to_string()],
            serde_json::json!({ "path": args.output.display().to_string() }),
        );
        println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
    } else {
        println!(
            "Initialized starter tare specification: {}",
            args.output.display()
        );
    }
    ExitCode::SUCCESS
}

fn handle_profile(args: ProfileArgs) -> ExitCode {
    let spec = match TareSpec::from_file(&args.spec) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Spec error: {e}");
            return ExitCode::from(2);
        }
    };

    let tasks = match load_task_runs(&args.traces) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Traces loading error: {e}");
            return ExitCode::from(2);
        }
    };

    let profile = profile_dataset(&spec, &tasks);

    let mut artifacts = Vec::new();
    if let Some(ledger_path) = args.ledger {
        let ledger = TareLedger::create_from_tasks(&tasks, &spec);
        if let Err(e) = ledger.save_to_file(&ledger_path) {
            eprintln!("Failed to save ledger: {e}");
            return ExitCode::from(2);
        }
        artifacts.push(ledger_path.display().to_string());
    }

    if args.json {
        let env = HarnessEnvelope::success(
            format!(
                "Profiled {} tasks: pass rate {:.1}%, CPST ${:.4}, p95 {:.0}ms",
                profile.total_tasks,
                profile.pass_rate * 100.0,
                profile.costs.cpst_usd.unwrap_or(0.0),
                profile.task_latencies.p95_ms
            ),
            artifacts,
            profile,
        );
        println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
    } else {
        println!("{}", render_terminal_profile_table(&profile));
    }

    ExitCode::SUCCESS
}

fn handle_compare(args: CompareArgs) -> ExitCode {
    let spec = match TareSpec::from_file(&args.spec) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Spec error: {e}");
            return ExitCode::from(2);
        }
    };

    let base_tasks = match load_task_runs(&args.baseline) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Baseline traces error: {e}");
            return ExitCode::from(2);
        }
    };

    let cand_tasks = match load_task_runs(&args.candidate) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Candidate traces error: {e}");
            return ExitCode::from(2);
        }
    };

    let report = match compare_runs(&spec, &base_tasks, &cand_tasks) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Comparison error: {e}");
            return ExitCode::from(2);
        }
    };

    let econ =
        ProductionEconomics::calculate(&spec, &report.baseline_profile, &report.candidate_profile);

    if args.json {
        let status = if report.verdict.is_ship_approved() {
            EnvelopeStatus::Success
        } else {
            EnvelopeStatus::Error
        };
        let env = HarnessEnvelope {
            status,
            summary: format!(
                "Comparative metrology finished: verdict {} (CPST delta {:+.1}%, p95 delta {:+.1}%)",
                report.verdict.display_badge(),
                report.cpst_delta_pct,
                report.p95_delta_pct
            ),
            next_actions: if report.verdict.is_ship_approved() {
                vec!["Approved for production deployment".to_string()]
            } else {
                vec!["Investigate regressions or budget breaches before promotion".to_string()]
            },
            artifacts: Vec::new(),
            data: report.clone(),
        };
        println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
    } else {
        match args.format.to_lowercase().as_str() {
            "markdown" | "md" => {
                println!("{}", render_markdown_summary(&report, Some(&econ)));
            }
            "mermaid" => {
                println!("{}", render_mermaid_diagram(&report));
            }
            _ => {
                println!("{}", render_terminal_comparison_table(&report, Some(&econ)));
            }
        }
    }

    if report.verdict.is_ship_approved() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn handle_gate(args: GateArgs) -> ExitCode {
    let spec = match TareSpec::from_file(&args.spec) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Spec error: {e}");
            return ExitCode::from(2);
        }
    };

    let base_tasks = match load_task_runs(&args.baseline) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Baseline traces error: {e}");
            return ExitCode::from(2);
        }
    };

    let cand_tasks = match load_task_runs(&args.candidate) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Candidate traces error: {e}");
            return ExitCode::from(2);
        }
    };

    let report = match compare_runs(&spec, &base_tasks, &cand_tasks) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Gate comparison failed: {e}");
            return ExitCode::from(2);
        }
    };

    let econ =
        ProductionEconomics::calculate(&spec, &report.baseline_profile, &report.candidate_profile);

    let mut artifacts = Vec::new();

    if let Some(md_path) = args.markdown {
        let md = render_markdown_summary(&report, Some(&econ));
        if let Err(e) = fs::write(&md_path, md) {
            eprintln!("Failed to write Markdown summary: {e}");
            return ExitCode::from(2);
        }
        artifacts.push(md_path.display().to_string());
    }

    if let Some(otel_path) = args.otel {
        let otel_json = generate_otel_spans(&report, Some(&econ));
        if let Err(e) = fs::write(
            &otel_path,
            serde_json::to_string_pretty(&otel_json).unwrap_or_default(),
        ) {
            eprintln!("Failed to write OTel spans: {e}");
            return ExitCode::from(2);
        }
        artifacts.push(otel_path.display().to_string());
    }

    if args.json {
        let status = if report.verdict.is_ship_approved() {
            EnvelopeStatus::Success
        } else {
            EnvelopeStatus::Error
        };
        let env = HarnessEnvelope {
            status,
            summary: format!(
                "Gate evaluation decision: {} (CPST delta {:+.1}%, p95 delta {:+.1}%)",
                report.verdict.display_badge(),
                report.cpst_delta_pct,
                report.p95_delta_pct
            ),
            next_actions: if report.verdict.is_ship_approved() {
                vec!["Gate passed; proceed with promotion".to_string()]
            } else {
                vec!["Gate failed closed; block promotion".to_string()]
            },
            artifacts,
            data: report.clone(),
        };
        println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
    } else {
        println!("{}", render_terminal_comparison_table(&report, Some(&econ)));
    }

    if report.verdict.is_ship_approved() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn handle_ledger(args: LedgerArgs) -> ExitCode {
    if args.verify {
        match TareLedger::load_and_verify(&args.output) {
            Ok(ledger) => {
                if args.json {
                    let env = HarnessEnvelope::success(
                        format!(
                            "Ledger verified with {} entries; head hash {}",
                            ledger.total_entries, ledger.head_hash
                        ),
                        vec![args.output.display().to_string()],
                        serde_json::json!({ "verified": true, "total_entries": ledger.total_entries, "head_hash": ledger.head_hash }),
                    );
                    println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
                } else {
                    println!(
                        "Ledger {} cryptographically verified ({} entries, Head Merkle SHA-256: {})",
                        args.output.display(),
                        ledger.total_entries,
                        ledger.head_hash
                    );
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Ledger verification failed: {e}");
                ExitCode::from(1)
            }
        }
    } else {
        let spec = match TareSpec::from_file(&args.spec) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Spec error: {e}");
                return ExitCode::from(2);
            }
        };

        let tasks = match load_task_runs(&args.traces) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Traces loading error: {e}");
                return ExitCode::from(2);
            }
        };

        let ledger = TareLedger::create_from_tasks(&tasks, &spec);
        if let Err(e) = ledger.save_to_file(&args.output) {
            eprintln!("Failed to save ledger: {e}");
            return ExitCode::from(2);
        }

        if args.json {
            let env = HarnessEnvelope::success(
                format!(
                    "Created ledger with {} entries; head hash {}",
                    ledger.total_entries, ledger.head_hash
                ),
                vec![args.output.display().to_string()],
                ledger,
            );
            println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
        } else {
            println!(
                "Created task provenance ledger {} with {} entries (Head Merkle SHA-256: {})",
                args.output.display(),
                ledger.total_entries,
                ledger.head_hash
            );
        }
        ExitCode::SUCCESS
    }
}

fn handle_lock(args: LockArgs) -> ExitCode {
    let spec = match TareSpec::from_file(&args.spec) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Spec error: {e}");
            return ExitCode::from(2);
        }
    };

    let lockfile = match TareLockfile::create(&args.spec, &spec) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Lockfile creation error: {e}");
            return ExitCode::from(2);
        }
    };

    if let Err(e) = lockfile.save_to_file(&args.output) {
        eprintln!("Failed to save lockfile: {e}");
        return ExitCode::from(2);
    }

    if args.json {
        let env = HarnessEnvelope::success(
            format!("Locked specification to {}", args.output.display()),
            vec![args.output.display().to_string()],
            lockfile,
        );
        println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
    } else {
        println!(
            "Generated cryptographic lockfile: {}",
            args.output.display()
        );
        println!("Overall Lock Digest: {}", lockfile.overall_lock_digest);
    }
    ExitCode::SUCCESS
}

fn handle_verify(args: VerifyArgs) -> ExitCode {
    let spec = match TareSpec::from_file(&args.spec) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Spec error: {e}");
            return ExitCode::from(2);
        }
    };

    let lockfile = match TareLockfile::load_from_file(&args.lock) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Lockfile load error: {e}");
            return ExitCode::from(2);
        }
    };

    if let Err(e) = lockfile.verify(&args.spec, &spec) {
        eprintln!("Lockfile verification failed: {e}");
        if args.json {
            let env = HarnessEnvelope::error(
                format!("Verification failed: {e}"),
                vec!["Re-lock specification with `tare lock`".to_string()],
                vec![args.lock.display().to_string()],
                serde_json::json!({ "verified": false, "error": e.to_string() }),
            );
            println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
        }
        return ExitCode::from(1);
    }

    if args.json {
        let env = HarnessEnvelope::success(
            "Cryptographic lockfile verified successfully",
            vec![args.lock.display().to_string()],
            serde_json::json!({ "verified": true, "overall_digest": lockfile.overall_lock_digest }),
        );
        println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
    } else {
        println!(
            "Lockfile {} is cryptographically verified against {}",
            args.lock.display(),
            args.spec.display()
        );
    }
    ExitCode::SUCCESS
}

fn handle_audit(args: AuditArgs) -> ExitCode {
    let report = audit_repository(&args.root);

    if args.json {
        let status = if report.passed {
            EnvelopeStatus::Success
        } else {
            EnvelopeStatus::Error
        };
        let env = HarnessEnvelope {
            status,
            summary: format!(
                "Repository hygiene audit: {} of {} gates passed",
                report.passed_gates, report.total_gates
            ),
            next_actions: if report.passed {
                Vec::new()
            } else {
                vec!["Fix failing repository hygiene gates".to_string()]
            },
            artifacts: Vec::new(),
            data: report.clone(),
        };
        println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
    } else {
        println!("============================================================");
        println!("               SOFT FACTORY REPO HYGIENE AUDIT              ");
        println!("============================================================");
        for gate in &report.gates {
            let status = if gate.passed { "[PASS]" } else { "[FAIL]" };
            println!("{:<50} {}", gate.name, status);
            println!("  -> {}", gate.details);
        }
        println!("------------------------------------------------------------");
        println!(
            "Overall Result: {} / {} gates passed",
            report.passed_gates, report.total_gates
        );
        if report.passed {
            println!("Status: ALL REPOSITORY HYGIENE GATES PASSED");
        } else {
            println!("Status: HYGIENE GATES FAILED");
        }
        println!("============================================================");
    }

    if report.passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
