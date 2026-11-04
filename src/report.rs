use crate::economics::ProductionEconomics;
use crate::metrics::RunProfile;
use crate::pareto::ComparisonReport;
use serde_json::json;

pub fn render_reference_physics_table() -> String {
    let mut out = String::new();
    out.push_str("========================================================================================================\n");
    out.push_str("                  TARE METROLOGY REFERENCE: COST PER SUCCESSFUL TASK & PARETO GATING                    \n");
    out.push_str("========================================================================================================\n");
    out.push_str("Scenario                               Quality   Tokens/Req   Cost/Call   Cost/Success (CPST)   p95 Latency   Verdict\n");
    out.push_str("--------------------------------------------------------------------------------------------------------\n");
    out.push_str("1. Baseline Production Agent            80.0%       1,850      $0.0360          $0.0450           1,840ms     [BASELINE]\n");
    out.push_str("2. Prompt Cache & Context Prune         80.0%         980      $0.0210          $0.0264 (-41%)    1,120ms     [SHIP: Quality-Flat Efficiency Win]\n");
    out.push_str("3. Cheap Model Swap (Vanity Trap)       32.0%       1,650      $0.0200 (-44%)   $0.0625 (+39%)    4,200ms     [BLOCKED: Vanity Call Cost Trap]\n");
    out.push_str("4. Pareto Breakthrough Model            90.0%       1,200      $0.0270          $0.0300 (-33%)    1,280ms     [SHIP: Pareto Dominant Win]\n");
    out.push_str("5. Unbounded Reasoning Loop             81.0%       4,800      $0.0910          $0.1123 (+150%)   6,800ms     [BLOCKED: Latency SLA Breach]\n");
    out.push_str("========================================================================================================\n");
    out.push_str("Key Ground Truth:\n");
    out.push_str("  - Tracking cost per call hides failure rate and retry blowup.\n");
    out.push_str("  - Cost Per Successful Task (CPST) = Gross Task Spend / Total Tasks Passed.\n");
    out.push_str("  - A change that keeps quality flat while cutting CPST 40% is a high-impact win that deserves to ship.\n");
    out
}

pub fn render_terminal_profile_table(profile: &RunProfile) -> String {
    let mut out = String::new();
    out.push_str(
        "================================================================================\n",
    );
    out.push_str(
        "                         TARE RUN PROFILE SCORECARD                             \n",
    );
    out.push_str(
        "================================================================================\n",
    );
    out.push_str(&format!(
        "Tasks Evaluated:   {:<6} | Passed: {:<6} | Failed: {:<6} | Pass Rate: {:.1}%\n",
        profile.total_tasks,
        profile.passed_tasks,
        profile.failed_tasks,
        profile.pass_rate * 100.0
    ));
    out.push_str(&format!(
        "Model Requests:    {:<6} | Mean Reqs/Task: {:.2} | Cache Hit Ratio: {:.1}%\n",
        profile.total_requests,
        profile.mean_requests_per_task,
        profile.tokens.prompt_cache_hit_rate * 100.0
    ));
    out.push_str(
        "--------------------------------------------------------------------------------\n",
    );
    out.push_str("TOKEN METROLOGY:\n");
    out.push_str(&format!(
        "  - Total Tokens:            {:<10} (Input: {}, Cached: {}, Write: {}, Out: {})\n",
        profile.tokens.total_tokens,
        profile.tokens.total_input_tokens,
        profile.tokens.total_cached_input_tokens,
        profile.tokens.total_cache_write_tokens,
        profile.tokens.total_output_tokens
    ));
    out.push_str(&format!(
        "  - Reasoning Tokens:        {:<10} ({:.1}% of total generation tokens)\n",
        profile.tokens.total_reasoning_tokens,
        profile.tokens.reasoning_token_share * 100.0
    ));
    out.push_str(&format!(
        "  - Mean Tokens/Request:     {:<10.1} | Mean Tokens/Task: {:.1}\n",
        profile.tokens.mean_tokens_per_request, profile.tokens.mean_tokens_per_task
    ));
    out.push_str(
        "--------------------------------------------------------------------------------\n",
    );
    out.push_str("COST METROLOGY:\n");
    out.push_str(&format!(
        "  - Total Spend:             ${:.4}\n",
        profile.costs.total_cost_usd
    ));
    out.push_str(&format!(
        "  - Cost Per Call (CPC):     ${:.4}\n",
        profile.costs.cpc_usd
    ));
    out.push_str(&format!(
        "  - Cost Per Attempt (CPTA): ${:.4}\n",
        profile.costs.cpta_usd
    ));
    out.push_str(&format!(
        "  - Cost Per Success (CPST): ${:.4} (Failure Tax Multiplier: {:.2}x)\n",
        profile.costs.cpst_usd.unwrap_or(0.0),
        profile.costs.failure_tax_multiplier
    ));
    out.push_str(&format!(
        "  - Failure Waste Spend:     ${:.4} ({:.1}% of total spend)\n",
        profile.costs.failure_waste_usd,
        if profile.costs.total_cost_usd > 0.0 {
            (profile.costs.failure_waste_usd / profile.costs.total_cost_usd) * 100.0
        } else {
            0.0
        }
    ));
    out.push_str(
        "--------------------------------------------------------------------------------\n",
    );
    out.push_str("LATENCY METROLOGY (Quantiles):\n");
    out.push_str(&format!(
        "  - Task Duration:           p50: {:.0}ms | p90: {:.0}ms | p95: {:.0}ms | p99: {:.0}ms | Mean: {:.0}ms\n",
        profile.task_latencies.p50_ms,
        profile.task_latencies.p90_ms,
        profile.task_latencies.p95_ms,
        profile.task_latencies.p99_ms,
        profile.task_latencies.mean_ms
    ));
    out.push_str(&format!(
        "  - Tail Amplification:      {:.2}x (ratio of p95 to p50 latency)\n",
        profile.task_latencies.tail_amplification_95_50
    ));
    if let Some(ttft) = &profile.ttft_quantiles {
        out.push_str(&format!(
            "  - Time To First Token:     p50: {:.0}ms | p95: {:.0}ms\n",
            ttft.p50_ms, ttft.p95_ms
        ));
    }
    out.push_str(
        "--------------------------------------------------------------------------------\n",
    );
    out.push_str("CATEGORY BREAKDOWN:\n");
    out.push_str("Category            Tasks   Passed   Pass Rate   CPST        CPC         Tok/Req   p95 Latency\n");
    for (name, cat) in &profile.categories {
        out.push_str(&format!(
            "{:<18}  {:<5}   {:<6}   {:>5.1}%      ${:<8.4}  ${:<8.4}  {:<7.0}   {:.0}ms\n",
            name,
            cat.total_tasks,
            cat.passed_tasks,
            cat.pass_rate * 100.0,
            cat.cpst_usd.unwrap_or(0.0),
            cat.cpc_usd,
            cat.mean_tokens_per_request,
            cat.p95_ms
        ));
    }
    out.push_str(
        "================================================================================\n",
    );
    out
}

pub fn render_terminal_comparison_table(
    report: &ComparisonReport,
    econ: Option<&ProductionEconomics>,
) -> String {
    let mut out = String::new();
    out.push_str("========================================================================================================\n");
    out.push_str("                         TARE COMPARATIVE METROLOGY & PARETO GATE SCORECARD                             \n");
    out.push_str("========================================================================================================\n");
    out.push_str(&format!("Suite: {}\n", report.suite_name));
    out.push_str(&format!(
        "Gating Verdict: {}\n",
        report.verdict.display_badge()
    ));
    for reason in &report.diagnostic_reasons {
        out.push_str(&format!("  * {}\n", reason));
    }
    out.push_str("--------------------------------------------------------------------------------------------------------\n");
    out.push_str("Metric                         Baseline          Candidate         Delta             95% Bootstrap CI\n");
    out.push_str("--------------------------------------------------------------------------------------------------------\n");
    out.push_str(&format!(
        "Task Quality (Pass Rate)       {:<15.1}%  {:<15.1}%  {:>+6.1}%           [{:+.1}%, {:+.1}%]\n",
        report.baseline_profile.pass_rate * 100.0,
        report.candidate_profile.pass_rate * 100.0,
        (report.candidate_profile.pass_rate - report.baseline_profile.pass_rate) * 100.0,
        report.quality_ci.ci_lower * 100.0,
        report.quality_ci.ci_upper * 100.0
    ));
    out.push_str(&format!(
        "Cost Per Success (CPST)        ${:<14.4}  ${:<14.4}  {:>+6.1}%           [{:+.1}%, {:+.1}%]\n",
        report.baseline_profile.costs.cpst_usd.unwrap_or(0.0),
        report.candidate_profile.costs.cpst_usd.unwrap_or(0.0),
        report.cpst_delta_pct,
        report.cpst_pct_ci.ci_lower,
        report.cpst_pct_ci.ci_upper
    ));
    out.push_str(&format!(
        "Cost Per Call (CPC)            ${:<14.4}  ${:<14.4}  {:>+6.1}%\n",
        report.baseline_profile.costs.cpc_usd,
        report.candidate_profile.costs.cpc_usd,
        report.cpc_delta_pct
    ));
    out.push_str(&format!(
        "Mean Tokens/Request            {:<15.0}  {:<15.0}  {:>+6.1}%           [{:+.1}%, {:+.1}%]\n",
        report.baseline_profile.tokens.mean_tokens_per_request,
        report.candidate_profile.tokens.mean_tokens_per_request,
        ((report.candidate_profile.tokens.mean_tokens_per_request - report.baseline_profile.tokens.mean_tokens_per_request)
            / report.baseline_profile.tokens.mean_tokens_per_request) * 100.0,
        report.tokens_pct_ci.ci_lower,
        report.tokens_pct_ci.ci_upper
    ));
    out.push_str(&format!(
        "Prompt Cache Hit Rate          {:<15.1}%  {:<15.1}%  {:>+6.1}%\n",
        report.baseline_profile.tokens.prompt_cache_hit_rate * 100.0,
        report.candidate_profile.tokens.prompt_cache_hit_rate * 100.0,
        (report.candidate_profile.tokens.prompt_cache_hit_rate
            - report.baseline_profile.tokens.prompt_cache_hit_rate)
            * 100.0
    ));
    out.push_str(&format!(
        "Median Latency (p50)           {:<13.0}ms  {:<13.0}ms  {:>+6.1}%\n",
        report.baseline_profile.task_latencies.p50_ms,
        report.candidate_profile.task_latencies.p50_ms,
        report.p50_delta_pct
    ));
    out.push_str(&format!(
        "Tail Latency (p95)             {:<13.0}ms  {:<13.0}ms  {:>+6.1}%           [{:+.1}%, {:+.1}%]\n",
        report.baseline_profile.task_latencies.p95_ms,
        report.candidate_profile.task_latencies.p95_ms,
        report.p95_delta_pct,
        report.p95_pct_ci.ci_lower,
        report.p95_pct_ci.ci_upper
    ));
    out.push_str(&format!(
        "Failure Waste Rate             {:<15.1}%  {:<15.1}%  {:>+6.1}%\n",
        (report.baseline_profile.costs.failure_waste_usd
            / report.baseline_profile.costs.total_cost_usd.max(0.001))
            * 100.0,
        (report.candidate_profile.costs.failure_waste_usd
            / report.candidate_profile.costs.total_cost_usd.max(0.001))
            * 100.0,
        ((report.candidate_profile.costs.failure_waste_usd
            / report.candidate_profile.costs.total_cost_usd.max(0.001))
            - (report.baseline_profile.costs.failure_waste_usd
                / report.baseline_profile.costs.total_cost_usd.max(0.001)))
            * 100.0
    ));
    out.push_str("--------------------------------------------------------------------------------------------------------\n");
    out.push_str("CATEGORY GATING AUDIT:\n");
    out.push_str("Category            Baseline Pass   Candidate Pass   Delta    Baseline CPST   Candidate CPST   Delta    Gate Status\n");
    for (name, cat) in &report.categories {
        out.push_str(&format!(
            "{:<18}  {:>5.1}%           {:>5.1}%         {:>+5.1}%   ${:<13.4}   ${:<13.4}  {:>+6.1}%   {}\n",
            name,
            cat.baseline_pass_rate * 100.0,
            cat.candidate_pass_rate * 100.0,
            cat.pass_rate_delta * 100.0,
            cat.baseline_cpst_usd.unwrap_or(0.0),
            cat.candidate_cpst_usd.unwrap_or(0.0),
            cat.cpst_delta_pct.unwrap_or(0.0),
            if cat.passed_gate { "[PASS]" } else { "[FAIL]" }
        ));
    }
    if let Some(e) = econ {
        out.push_str("--------------------------------------------------------------------------------------------------------\n");
        out.push_str("PROJECTED PRODUCTION SCALE ECONOMICS:\n");
        out.push_str(&format!(
            "  - Daily Successful Tasks:      {}\n",
            e.daily_successful_tasks
        ));
        out.push_str(&format!(
            "  - Daily API Spend Reduction:   ${:.2} / day\n",
            e.daily_api_savings_usd
        ));
        out.push_str(&format!(
            "  - Annual API Spend Reduction:  ${:.2} / year\n",
            e.annual_api_savings_usd
        ));
        out.push_str(&format!(
            "  - Annual Failure Waste Cut:    ${:.2} / year\n",
            e.annual_failure_waste_savings_usd
        ));
        out.push_str(&format!(
            "  - Annual Latency Value:        ${:.2} / year\n",
            e.annual_latency_productivity_value_usd
        ));
        out.push_str(&format!(
            "  - Total Annual Net Impact:     ${:.2} / year\n",
            e.total_annual_economic_impact_usd
        ));
    }
    out.push_str("========================================================================================================\n");
    out
}

pub fn render_markdown_summary(
    report: &ComparisonReport,
    econ: Option<&ProductionEconomics>,
) -> String {
    let mut md = String::new();
    md.push_str(&format!(
        "## Tare Evaluation Gate: {}\n\n",
        report.verdict.display_badge()
    ));
    md.push_str(&format!(
        "**Suite:** `{}` | **Result:** {}\n\n",
        report.suite_name,
        report.verdict.display_badge()
    ));
    md.push_str("### Executive Summary\n\n");
    for reason in &report.diagnostic_reasons {
        md.push_str(&format!("- {}\n", reason));
    }
    md.push_str("\n### First-Class Metrics Comparison\n\n");
    md.push_str("| Metric | Baseline | Candidate | Delta | 95% Bootstrap CI |\n");
    md.push_str("|---|:---:|:---:|:---:|:---:|\n");
    md.push_str(&format!(
        "| **Quality (Pass Rate)** | {:.1}% | {:.1}% | **{:+.1}%** | [{:+.1}%, {:+.1}%] |\n",
        report.baseline_profile.pass_rate * 100.0,
        report.candidate_profile.pass_rate * 100.0,
        (report.candidate_profile.pass_rate - report.baseline_profile.pass_rate) * 100.0,
        report.quality_ci.ci_lower * 100.0,
        report.quality_ci.ci_upper * 100.0
    ));
    md.push_str(&format!(
        "| **Cost Per Success (CPST)** | ${:.4} | ${:.4} | **{:+.1}%** | [{:+.1}%, {:+.1}%] |\n",
        report.baseline_profile.costs.cpst_usd.unwrap_or(0.0),
        report.candidate_profile.costs.cpst_usd.unwrap_or(0.0),
        report.cpst_delta_pct,
        report.cpst_pct_ci.ci_lower,
        report.cpst_pct_ci.ci_upper
    ));
    md.push_str(&format!(
        "| Cost Per Call (CPC) | ${:.4} | ${:.4} | {:+.1}% | - |\n",
        report.baseline_profile.costs.cpc_usd,
        report.candidate_profile.costs.cpc_usd,
        report.cpc_delta_pct
    ));
    md.push_str(&format!(
        "| Mean Tokens/Request | {:.0} | {:.0} | {:+.1}% | [{:+.1}%, {:+.1}%] |\n",
        report.baseline_profile.tokens.mean_tokens_per_request,
        report.candidate_profile.tokens.mean_tokens_per_request,
        ((report.candidate_profile.tokens.mean_tokens_per_request
            - report.baseline_profile.tokens.mean_tokens_per_request)
            / report.baseline_profile.tokens.mean_tokens_per_request)
            * 100.0,
        report.tokens_pct_ci.ci_lower,
        report.tokens_pct_ci.ci_upper
    ));
    md.push_str(&format!(
        "| Tail Latency (p95) | {:.0}ms | {:.0}ms | **{:+.1}%** | [{:+.1}%, {:+.1}%] |\n",
        report.baseline_profile.task_latencies.p95_ms,
        report.candidate_profile.task_latencies.p95_ms,
        report.p95_delta_pct,
        report.p95_pct_ci.ci_lower,
        report.p95_pct_ci.ci_upper
    ));

    if let Some(e) = econ {
        md.push_str("\n### Production Economics at Scale\n\n");
        md.push_str(&format!(
            "- **Daily Tasks:** {} successful tasks/day\n",
            e.daily_successful_tasks
        ));
        md.push_str(&format!(
            "- **Annual API Spend Savings:** ${:.2} / year\n",
            e.annual_api_savings_usd
        ));
        md.push_str(&format!(
            "- **Annual Failure Waste Cut:** ${:.2} / year\n",
            e.annual_failure_waste_savings_usd
        ));
        md.push_str(&format!(
            "- **Total Annual Economic Impact:** **${:.2} / year**\n",
            e.total_annual_economic_impact_usd
        ));
    }
    md
}

pub fn render_mermaid_diagram(report: &ComparisonReport) -> String {
    let mut m = String::new();
    m.push_str("flowchart TD\n");
    m.push_str(&format!(
        "    B[\"Baseline: {:.1}% Quality\\n${:.4} CPST | p95: {:.0}ms\"]\n",
        report.baseline_profile.pass_rate * 100.0,
        report.baseline_profile.costs.cpst_usd.unwrap_or(0.0),
        report.baseline_profile.task_latencies.p95_ms
    ));
    m.push_str(&format!(
        "    C[\"Candidate: {:.1}% Quality\\n${:.4} CPST | p95: {:.0}ms\"]\n",
        report.candidate_profile.pass_rate * 100.0,
        report.candidate_profile.costs.cpst_usd.unwrap_or(0.0),
        report.candidate_profile.task_latencies.p95_ms
    ));
    m.push_str(&format!(
        "    B -->|CPST Delta: {:+.1}%\\np95 Delta: {:+.1}%| C\n",
        report.cpst_delta_pct, report.p95_delta_pct
    ));
    m.push_str(&format!(
        "    C --> V[\"Verdict: {}\"]\n",
        report.verdict.display_badge()
    ));
    m
}

pub fn generate_otel_spans(
    report: &ComparisonReport,
    econ: Option<&ProductionEconomics>,
) -> serde_json::Value {
    let mut hasher = sha2::Sha256::default();
    sha2::Digest::update(&mut hasher, report.suite_name.as_bytes());
    let trace_id = format!("{:x}", sha2::Digest::finalize(hasher));
    json!([
        {
            "trace_id": &trace_id[..32],
            "span_id": "tare-gate-eval",
            "name": "tare.gate_evaluation",
            "attributes": {
                "eval.verdict": format!("{:?}", report.verdict),
                "eval.is_ship_approved": report.verdict.is_ship_approved(),
                "eval.baseline_cpst_usd": report.baseline_profile.costs.cpst_usd,
                "eval.candidate_cpst_usd": report.candidate_profile.costs.cpst_usd,
                "eval.cpst_delta_pct": report.cpst_delta_pct,
                "eval.baseline_p95_ms": report.baseline_profile.task_latencies.p95_ms,
                "eval.candidate_p95_ms": report.candidate_profile.task_latencies.p95_ms,
                "eval.p95_delta_pct": report.p95_delta_pct,
                "eval.total_annual_impact_usd": econ.map(|e| e.total_annual_economic_impact_usd)
            }
        }
    ])
}
