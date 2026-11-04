use crate::ingest::TaskRunRecord;
use crate::metrics::{compute_task_cost, compute_task_duration_ms, RunProfile};
use crate::spec::TareSpec;
use crate::stats::{
    bootstrap_paired_deltas, check_tost_equivalence, exact_mcnemar_test, BootstrapCi,
    McNemarResult, PairedObservation,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TareVerdict {
    ParetoDominantWin,
    QualityFlatEfficiencyWin,
    CapabilityWinWithinBudget,
    NeutralWithinMargin,
    VanityCallCostTrapBlocked,
    CostBudgetBreachBlocked,
    LatencySlaBreachBlocked,
    QualityRegressionBlocked,
}

impl TareVerdict {
    pub fn is_ship_approved(&self) -> bool {
        matches!(
            self,
            TareVerdict::ParetoDominantWin
                | TareVerdict::QualityFlatEfficiencyWin
                | TareVerdict::CapabilityWinWithinBudget
                | TareVerdict::NeutralWithinMargin
        )
    }

    pub fn display_badge(&self) -> &'static str {
        match self {
            TareVerdict::ParetoDominantWin => "[SHIP: Pareto Dominant Win]",
            TareVerdict::QualityFlatEfficiencyWin => "[SHIP: Quality-Flat Efficiency Win]",
            TareVerdict::CapabilityWinWithinBudget => "[SHIP: Capability Win Within Budget]",
            TareVerdict::NeutralWithinMargin => "[NEUTRAL: Within Tolerance]",
            TareVerdict::VanityCallCostTrapBlocked => "[BLOCKED: Vanity Call Cost Trap]",
            TareVerdict::CostBudgetBreachBlocked => "[BLOCKED: Cost Budget Breach]",
            TareVerdict::LatencySlaBreachBlocked => "[BLOCKED: Latency SLA Breach]",
            TareVerdict::QualityRegressionBlocked => "[BLOCKED: Quality Regression]",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryComparison {
    pub category: String,
    pub baseline_pass_rate: f64,
    pub candidate_pass_rate: f64,
    pub pass_rate_delta: f64,
    pub baseline_cpst_usd: Option<f64>,
    pub candidate_cpst_usd: Option<f64>,
    pub cpst_delta_pct: Option<f64>,
    pub baseline_p95_ms: f64,
    pub candidate_p95_ms: f64,
    pub p95_delta_pct: f64,
    pub passed_gate: bool,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReport {
    pub suite_name: String,
    pub baseline_profile: RunProfile,
    pub candidate_profile: RunProfile,
    pub mcnemar: McNemarResult,
    pub is_tost_equivalent: bool,
    pub quality_ci: BootstrapCi,
    pub cpst_pct_ci: BootstrapCi,
    pub p95_pct_ci: BootstrapCi,
    pub tokens_pct_ci: BootstrapCi,
    pub cpc_delta_pct: f64,
    pub cpst_delta_pct: f64,
    pub p50_delta_pct: f64,
    pub p95_delta_pct: f64,
    pub categories: HashMap<String, CategoryComparison>,
    pub verdict: TareVerdict,
    pub diagnostic_reasons: Vec<String>,
}

pub fn compare_runs(
    spec: &TareSpec,
    baseline_tasks: &[TaskRunRecord],
    candidate_tasks: &[TaskRunRecord],
) -> Result<ComparisonReport, String> {
    let baseline_profile = crate::metrics::profile_dataset(spec, baseline_tasks);
    let candidate_profile = crate::metrics::profile_dataset(spec, candidate_tasks);

    let mut baseline_map: HashMap<String, &TaskRunRecord> = HashMap::new();
    for t in baseline_tasks {
        baseline_map.insert(t.task_id.clone(), t);
    }

    let mut pairs = Vec::with_capacity(candidate_tasks.len());
    let mut concordant_pass = 0;
    let mut concordant_fail = 0;
    let mut regressions = 0;
    let mut gains = 0;

    for cand in candidate_tasks {
        if let Some(base) = baseline_map.get(&cand.task_id) {
            let b_pricing = spec.get_pricing(&base.model);
            let c_pricing = spec.get_pricing(&cand.model);

            let b_cost = compute_task_cost(base, &b_pricing);
            let c_cost = compute_task_cost(cand, &c_pricing);

            let b_dur = compute_task_duration_ms(base);
            let c_dur = compute_task_duration_ms(cand);

            let b_toks: u64 = base.requests.iter().map(|r| r.total_tokens()).sum();
            let c_toks: u64 = cand.requests.iter().map(|r| r.total_tokens()).sum();

            if base.passed && cand.passed {
                concordant_pass += 1;
            } else if !base.passed && !cand.passed {
                concordant_fail += 1;
            } else if base.passed && !cand.passed {
                regressions += 1;
            } else {
                gains += 1;
            }

            pairs.push(PairedObservation {
                baseline_passed: base.passed,
                candidate_passed: cand.passed,
                baseline_cost: b_cost,
                candidate_cost: c_cost,
                baseline_duration_ms: b_dur,
                candidate_duration_ms: c_dur,
                baseline_tokens: b_toks,
                candidate_tokens: c_toks,
            });
        }
    }

    if pairs.is_empty() {
        return Err("No matching task_ids found between baseline and candidate".to_string());
    }

    let mcnemar = exact_mcnemar_test(
        concordant_pass,
        concordant_fail,
        regressions,
        gains,
        spec.gate.mcnemar_alpha,
    );

    let is_tost_equivalent = check_tost_equivalence(
        baseline_profile.pass_rate,
        candidate_profile.pass_rate,
        spec.gate.tost_equivalence_margin,
    );

    let (quality_ci, cpst_pct_ci, p95_pct_ci, tokens_pct_ci) =
        bootstrap_paired_deltas(&pairs, 3000, 0x1234_5678);

    let cpc_delta_pct = if baseline_profile.costs.cpc_usd > 0.000_001 {
        ((candidate_profile.costs.cpc_usd - baseline_profile.costs.cpc_usd)
            / baseline_profile.costs.cpc_usd)
            * 100.0
    } else {
        0.0
    };

    let cpst_delta_pct = match (
        baseline_profile.costs.cpst_usd,
        candidate_profile.costs.cpst_usd,
    ) {
        (Some(b), Some(c)) if b > 0.000_001 => ((c - b) / b) * 100.0,
        _ => 0.0,
    };

    let p50_delta_pct = if baseline_profile.task_latencies.p50_ms > 0.001 {
        ((candidate_profile.task_latencies.p50_ms - baseline_profile.task_latencies.p50_ms)
            / baseline_profile.task_latencies.p50_ms)
            * 100.0
    } else {
        0.0
    };

    let p95_delta_pct = if baseline_profile.task_latencies.p95_ms > 0.001 {
        ((candidate_profile.task_latencies.p95_ms - baseline_profile.task_latencies.p95_ms)
            / baseline_profile.task_latencies.p95_ms)
            * 100.0
    } else {
        0.0
    };

    let mut categories = HashMap::new();
    let mut category_regressed = false;
    let mut diagnostic_reasons = Vec::new();

    for (cat_name, base_cat) in &baseline_profile.categories {
        if let Some(cand_cat) = candidate_profile.categories.get(cat_name) {
            let pass_delta = cand_cat.pass_rate - base_cat.pass_rate;
            let mut passed_gate = true;
            let mut failure_reason = None;

            if let Some(spec_cat) = spec.category.get(cat_name) {
                if let Some(floor) = spec_cat.floor_pass_rate {
                    if cand_cat.pass_rate < floor {
                        passed_gate = false;
                        failure_reason = Some(format!(
                            "Pass rate {:.1}% below floor {:.1}%",
                            cand_cat.pass_rate * 100.0,
                            floor * 100.0
                        ));
                    }
                }
                let max_drop = spec_cat.max_drop.unwrap_or(spec.gate.max_quality_drop);
                if pass_delta < -max_drop {
                    passed_gate = false;
                    failure_reason = Some(format!(
                        "Pass rate drop {:.1}% exceeds max allowable drop {:.1}%",
                        pass_delta.abs() * 100.0,
                        max_drop * 100.0
                    ));
                }
            } else if pass_delta < -spec.gate.max_quality_drop {
                passed_gate = false;
                failure_reason = Some(format!(
                    "Pass rate drop {:.1}% exceeds default max drop {:.1}%",
                    pass_delta.abs() * 100.0,
                    spec.gate.max_quality_drop * 100.0
                ));
            }

            if !passed_gate {
                category_regressed = true;
                if let Some(ref r) = failure_reason {
                    diagnostic_reasons.push(format!("Category '{cat_name}': {r}"));
                }
            }

            let cat_cpst_delta = match (base_cat.cpst_usd, cand_cat.cpst_usd) {
                (Some(b), Some(c)) if b > 0.000_001 => Some(((c - b) / b) * 100.0),
                _ => None,
            };

            let cat_p95_delta = if base_cat.p95_ms > 0.001 {
                ((cand_cat.p95_ms - base_cat.p95_ms) / base_cat.p95_ms) * 100.0
            } else {
                0.0
            };

            categories.insert(
                cat_name.clone(),
                CategoryComparison {
                    category: cat_name.clone(),
                    baseline_pass_rate: base_cat.pass_rate,
                    candidate_pass_rate: cand_cat.pass_rate,
                    pass_rate_delta: pass_delta,
                    baseline_cpst_usd: base_cat.cpst_usd,
                    candidate_cpst_usd: cand_cat.cpst_usd,
                    cpst_delta_pct: cat_cpst_delta,
                    baseline_p95_ms: base_cat.p95_ms,
                    candidate_p95_ms: cand_cat.p95_ms,
                    p95_delta_pct: cat_p95_delta,
                    passed_gate,
                    failure_reason,
                },
            );
        }
    }

    let quality_significantly_worse = mcnemar.is_significant && regressions > gains;
    let quality_significantly_better = mcnemar.is_significant && gains > regressions;

    let below_global_floor = candidate_profile.pass_rate < spec.gate.min_quality_floor;

    let cpc_decreased = cpc_delta_pct < -10.0;
    let cpst_increased = cpst_delta_pct > 5.0;
    let is_vanity_trap = cpc_decreased && cpst_increased;

    let cpst_budget_breach = if let Some(max_cpst) = spec.budgets.max_cpst_usd {
        candidate_profile
            .costs
            .cpst_usd
            .map(|c| c > max_cpst)
            .unwrap_or(true)
    } else {
        false
    };

    let cpst_relative_breach = cpst_delta_pct > spec.gate.max_cpst_increase_pct;

    let p95_sla_breach = if let Some(max_p95) = spec.budgets.max_p95_ms {
        candidate_profile.task_latencies.p95_ms > max_p95
    } else {
        false
    };

    let p95_relative_breach = p95_delta_pct > spec.gate.max_p95_increase_pct;

    let verdict = if quality_significantly_worse || category_regressed || below_global_floor {
        if quality_significantly_worse {
            diagnostic_reasons.push(format!(
                "Exact McNemar test detected significant quality regression (p = {:.4}, regressions = {}, gains = {})",
                mcnemar.p_value, regressions, gains
            ));
        }
        if below_global_floor {
            diagnostic_reasons.push(format!(
                "Candidate pass rate {:.1}% is below global floor {:.1}%",
                candidate_profile.pass_rate * 100.0,
                spec.gate.min_quality_floor * 100.0
            ));
        }
        TareVerdict::QualityRegressionBlocked
    } else if is_vanity_trap {
        diagnostic_reasons.push(format!(
            "Vanity Call Cost Trap: Cost per call dropped {:.1}%, but Cost Per Successful Task (CPST) increased +{:.1}% due to failures and retries",
            cpc_delta_pct.abs(),
            cpst_delta_pct
        ));
        TareVerdict::VanityCallCostTrapBlocked
    } else if cpst_budget_breach || cpst_relative_breach {
        if cpst_budget_breach {
            diagnostic_reasons.push(format!(
                "Candidate CPST ${:.4} exceeds configured budget ceiling ${:.4}",
                candidate_profile.costs.cpst_usd.unwrap_or(0.0),
                spec.budgets.max_cpst_usd.unwrap_or(0.0)
            ));
        }
        if cpst_relative_breach {
            diagnostic_reasons.push(format!(
                "CPST increase +{:.1}% exceeds maximum allowable increase {:.1}%",
                cpst_delta_pct, spec.gate.max_cpst_increase_pct
            ));
        }
        TareVerdict::CostBudgetBreachBlocked
    } else if p95_sla_breach || p95_relative_breach {
        if p95_sla_breach {
            diagnostic_reasons.push(format!(
                "Candidate p95 latency {:.0}ms exceeds configured SLA ceiling {:.0}ms",
                candidate_profile.task_latencies.p95_ms,
                spec.budgets.max_p95_ms.unwrap_or(0.0)
            ));
        }
        if p95_relative_breach {
            diagnostic_reasons.push(format!(
                "Candidate p95 latency increase +{:.1}% exceeds maximum allowable increase {:.1}%",
                p95_delta_pct, spec.gate.max_p95_increase_pct
            ));
        }
        TareVerdict::LatencySlaBreachBlocked
    } else if quality_significantly_better {
        diagnostic_reasons.push(format!(
            "Quality improved significantly (McNemar p = {:.4}, gains = {}, regressions = {}) while staying within cost and latency budgets",
            mcnemar.p_value, gains, regressions
        ));
        if cpst_delta_pct < -5.0 && p95_delta_pct < -5.0 {
            TareVerdict::ParetoDominantWin
        } else {
            TareVerdict::CapabilityWinWithinBudget
        }
    } else if is_tost_equivalent
        && (cpst_delta_pct <= -spec.gate.efficiency_win_min_cpst_cut_pct
            || (p95_delta_pct <= -20.0 && cpst_delta_pct <= 0.0))
    {
        diagnostic_reasons.push(format!(
            "Quality held flat within TOST equivalence margin (+/-{:.1}%) while slashing CPST by {:.1}% (min target: {:.1}%) and p95 latency by {:.1}%",
            spec.gate.tost_equivalence_margin * 100.0,
            cpst_delta_pct.abs(),
            spec.gate.efficiency_win_min_cpst_cut_pct,
            p95_delta_pct.abs()
        ));
        TareVerdict::QualityFlatEfficiencyWin
    } else {
        diagnostic_reasons.push(
            "Quality, cost, and latency variations remain within normal statistical noise bounds"
                .to_string(),
        );
        TareVerdict::NeutralWithinMargin
    };

    Ok(ComparisonReport {
        suite_name: spec.suite.name.clone(),
        baseline_profile,
        candidate_profile,
        mcnemar,
        is_tost_equivalent,
        quality_ci,
        cpst_pct_ci,
        p95_pct_ci,
        tokens_pct_ci,
        cpc_delta_pct,
        cpst_delta_pct,
        p50_delta_pct,
        p95_delta_pct,
        categories,
        verdict,
        diagnostic_reasons,
    })
}
