use crate::ingest::TaskRunRecord;
use crate::spec::{ModelPricingConfig, TareSpec};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyQuantiles {
    pub min_ms: f64,
    pub p50_ms: f64,
    pub p90_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    pub mean_ms: f64,
    pub tail_amplification_95_50: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSummary {
    pub total_input_tokens: u64,
    pub total_cached_input_tokens: u64,
    pub total_cache_write_tokens: u64,
    pub total_output_tokens: u64,
    pub total_reasoning_tokens: u64,
    pub total_tokens: u64,
    pub mean_tokens_per_request: f64,
    pub mean_tokens_per_task: f64,
    pub prompt_cache_hit_rate: f64,
    pub reasoning_token_share: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    pub total_cost_usd: f64,
    pub cpc_usd: f64,
    pub cpta_usd: f64,
    pub cpst_usd: Option<f64>,
    pub failure_waste_usd: f64,
    pub failure_tax_multiplier: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryScorecard {
    pub category: String,
    pub total_tasks: usize,
    pub passed_tasks: usize,
    pub pass_rate: f64,
    pub wilson_ci_lower: f64,
    pub wilson_ci_upper: f64,
    pub cpst_usd: Option<f64>,
    pub cpc_usd: f64,
    pub mean_tokens_per_request: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunProfile {
    pub total_tasks: usize,
    pub passed_tasks: usize,
    pub failed_tasks: usize,
    pub pass_rate: f64,
    pub total_requests: usize,
    pub mean_requests_per_task: f64,
    pub tokens: TokenSummary,
    pub costs: CostSummary,
    pub task_latencies: LatencyQuantiles,
    pub request_latencies: LatencyQuantiles,
    pub ttft_quantiles: Option<LatencyQuantiles>,
    pub categories: BTreeMap<String, CategoryScorecard>,
}

pub fn compute_task_cost(task: &TaskRunRecord, pricing: &ModelPricingConfig) -> f64 {
    let mut total = 0.0;
    for req in &task.requests {
        if let Some(micros) = req.cost_usd_micros {
            total += (micros as f64) / 1_000_000.0;
        } else {
            let input_cost = (req.input_tokens as f64) * pricing.input_per_million / 1_000_000.0;
            let cache_read_cost =
                (req.cached_input_tokens as f64) * pricing.cache_read_per_million / 1_000_000.0;
            let cache_write_cost =
                (req.cache_write_tokens as f64) * pricing.cache_write_per_million / 1_000_000.0;
            let reasoning_price = pricing
                .reasoning_per_million
                .unwrap_or(pricing.output_per_million);
            let reasoning_cost = (req.reasoning_tokens as f64) * reasoning_price / 1_000_000.0;
            let output_cost = (req.output_tokens as f64) * pricing.output_per_million / 1_000_000.0;
            total += input_cost + cache_read_cost + cache_write_cost + reasoning_cost + output_cost;
        }
    }
    total
}

pub fn compute_task_duration_ms(task: &TaskRunRecord) -> f64 {
    if let Some(wall) = task.wall_clock_ms {
        wall
    } else {
        task.requests.iter().map(|r| r.duration_ms).sum()
    }
}

pub fn calculate_quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let n = sorted.len();
    let index = (n as f64 - 1.0) * q;
    let low = index.floor() as usize;
    let high = index.ceil() as usize;
    if low == high {
        sorted[low]
    } else {
        let frac = index - (low as f64);
        sorted[low] * (1.0 - frac) + sorted[high] * frac
    }
}

pub fn compute_latency_quantiles(durations: &[f64]) -> LatencyQuantiles {
    if durations.is_empty() {
        return LatencyQuantiles {
            min_ms: 0.0,
            p50_ms: 0.0,
            p90_ms: 0.0,
            p95_ms: 0.0,
            p99_ms: 0.0,
            max_ms: 0.0,
            mean_ms: 0.0,
            tail_amplification_95_50: 1.0,
        };
    }

    let mut sorted = durations.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let min_ms = sorted.first().copied().unwrap_or(0.0);
    let max_ms = sorted.last().copied().unwrap_or(0.0);
    let sum: f64 = sorted.iter().sum();
    let mean_ms = sum / (sorted.len() as f64);

    let p50_ms = calculate_quantile(&sorted, 0.50);
    let p90_ms = calculate_quantile(&sorted, 0.90);
    let p95_ms = calculate_quantile(&sorted, 0.95);
    let p99_ms = calculate_quantile(&sorted, 0.99);

    let tail_amplification_95_50 = if p50_ms > 0.001 { p95_ms / p50_ms } else { 1.0 };

    LatencyQuantiles {
        min_ms,
        p50_ms,
        p90_ms,
        p95_ms,
        p99_ms,
        max_ms,
        mean_ms,
        tail_amplification_95_50,
    }
}

pub fn compute_wilson_ci(k: usize, n: usize, z: f64) -> (f64, f64) {
    if n == 0 {
        return (0.0, 0.0);
    }
    let p_hat = (k as f64) / (n as f64);
    let n_f = n as f64;
    let z2 = z * z;
    let denom = 1.0 + z2 / n_f;
    let center = (p_hat + z2 / (2.0 * n_f)) / denom;
    let margin = (z / denom) * ((p_hat * (1.0 - p_hat) / n_f) + (z2 / (4.0 * n_f * n_f))).sqrt();
    let lower = (center - margin).max(0.0);
    let upper = (center + margin).min(1.0);
    (lower, upper)
}

pub fn profile_dataset(spec: &TareSpec, tasks: &[TaskRunRecord]) -> RunProfile {
    let total_tasks = tasks.len();
    let passed_tasks = tasks.iter().filter(|t| t.passed).count();
    let failed_tasks = total_tasks.saturating_sub(passed_tasks);
    let pass_rate = if total_tasks > 0 {
        (passed_tasks as f64) / (total_tasks as f64)
    } else {
        0.0
    };

    let mut total_requests = 0;
    let mut total_in = 0u64;
    let mut total_cached = 0u64;
    let mut total_write = 0u64;
    let mut total_out = 0u64;
    let mut total_reasoning = 0u64;

    let mut task_latencies_vec = Vec::with_capacity(total_tasks);
    let mut request_latencies_vec = Vec::new();
    let mut ttft_vec = Vec::new();

    let mut total_cost_usd = 0.0;
    let mut failure_waste_usd = 0.0;
    let mut success_costs_usd = Vec::new();

    let mut by_category: BTreeMap<String, Vec<&TaskRunRecord>> = BTreeMap::new();

    for task in tasks {
        let pricing = spec.get_pricing(&task.model);
        let task_cost = compute_task_cost(task, &pricing);
        total_cost_usd += task_cost;

        if task.passed {
            success_costs_usd.push(task_cost);
        } else {
            failure_waste_usd += task_cost;
        }

        task_latencies_vec.push(compute_task_duration_ms(task));

        for req in &task.requests {
            total_requests += 1;
            total_in = total_in.saturating_add(req.input_tokens);
            total_cached = total_cached.saturating_add(req.cached_input_tokens);
            total_write = total_write.saturating_add(req.cache_write_tokens);
            total_out = total_out.saturating_add(req.output_tokens);
            total_reasoning = total_reasoning.saturating_add(req.reasoning_tokens);

            request_latencies_vec.push(req.duration_ms);
            if let Some(ttft) = req.ttft_ms {
                ttft_vec.push(ttft);
            }
        }

        by_category
            .entry(task.category.clone())
            .or_default()
            .push(task);
    }

    let all_tokens = total_in
        .saturating_add(total_cached)
        .saturating_add(total_write)
        .saturating_add(total_out)
        .saturating_add(total_reasoning);

    let all_prompt_tokens = total_in
        .saturating_add(total_cached)
        .saturating_add(total_write);

    let prompt_cache_hit_rate = if all_prompt_tokens > 0 {
        (total_cached as f64) / (all_prompt_tokens as f64)
    } else {
        0.0
    };

    let reasoning_token_share = if all_tokens > 0 {
        (total_reasoning as f64) / (all_tokens as f64)
    } else {
        0.0
    };

    let mean_tokens_per_request = if total_requests > 0 {
        (all_tokens as f64) / (total_requests as f64)
    } else {
        0.0
    };

    let mean_tokens_per_task = if total_tasks > 0 {
        (all_tokens as f64) / (total_tasks as f64)
    } else {
        0.0
    };

    let tokens = TokenSummary {
        total_input_tokens: total_in,
        total_cached_input_tokens: total_cached,
        total_cache_write_tokens: total_write,
        total_output_tokens: total_out,
        total_reasoning_tokens: total_reasoning,
        total_tokens: all_tokens,
        mean_tokens_per_request,
        mean_tokens_per_task,
        prompt_cache_hit_rate,
        reasoning_token_share,
    };

    let cpc_usd = if total_requests > 0 {
        total_cost_usd / (total_requests as f64)
    } else {
        0.0
    };

    let cpta_usd = if total_tasks > 0 {
        total_cost_usd / (total_tasks as f64)
    } else {
        0.0
    };

    let cpst_usd = if passed_tasks > 0 {
        Some(total_cost_usd / (passed_tasks as f64))
    } else {
        None
    };

    let mean_success_cost = if !success_costs_usd.is_empty() {
        let sum: f64 = success_costs_usd.iter().sum();
        sum / (success_costs_usd.len() as f64)
    } else {
        0.0
    };

    let failure_tax_multiplier = if mean_success_cost > 0.000_001 {
        cpst_usd.unwrap_or(0.0) / mean_success_cost
    } else if passed_tasks == 0 && total_tasks > 0 {
        99.99
    } else {
        1.0
    };

    let costs = CostSummary {
        total_cost_usd,
        cpc_usd,
        cpta_usd,
        cpst_usd,
        failure_waste_usd,
        failure_tax_multiplier,
    };

    let task_latencies = compute_latency_quantiles(&task_latencies_vec);
    let request_latencies = compute_latency_quantiles(&request_latencies_vec);
    let ttft_quantiles = if !ttft_vec.is_empty() {
        Some(compute_latency_quantiles(&ttft_vec))
    } else {
        None
    };

    let mut categories = BTreeMap::new();
    for (cat_name, cat_tasks) in by_category {
        let c_total = cat_tasks.len();
        let c_passed = cat_tasks.iter().filter(|t| t.passed).count();
        let c_pass_rate = if c_total > 0 {
            (c_passed as f64) / (c_total as f64)
        } else {
            0.0
        };
        let (wilson_lower, wilson_upper) = compute_wilson_ci(c_passed, c_total, 1.96);

        let mut c_cost = 0.0;
        let mut c_reqs = 0;
        let mut c_tokens = 0u64;
        let mut c_latencies = Vec::new();

        for t in &cat_tasks {
            let pricing = spec.get_pricing(&t.model);
            c_cost += compute_task_cost(t, &pricing);
            c_latencies.push(compute_task_duration_ms(t));
            for r in &t.requests {
                c_reqs += 1;
                c_tokens = c_tokens.saturating_add(r.total_tokens());
            }
        }

        let c_cpst = if c_passed > 0 {
            Some(c_cost / (c_passed as f64))
        } else {
            None
        };

        let c_cpc = if c_reqs > 0 {
            c_cost / (c_reqs as f64)
        } else {
            0.0
        };

        let c_mean_tok = if c_reqs > 0 {
            (c_tokens as f64) / (c_reqs as f64)
        } else {
            0.0
        };

        let c_lats = compute_latency_quantiles(&c_latencies);

        categories.insert(
            cat_name.clone(),
            CategoryScorecard {
                category: cat_name,
                total_tasks: c_total,
                passed_tasks: c_passed,
                pass_rate: c_pass_rate,
                wilson_ci_lower: wilson_lower,
                wilson_ci_upper: wilson_upper,
                cpst_usd: c_cpst,
                cpc_usd: c_cpc,
                mean_tokens_per_request: c_mean_tok,
                p50_ms: c_lats.p50_ms,
                p95_ms: c_lats.p95_ms,
            },
        );
    }

    RunProfile {
        total_tasks,
        passed_tasks,
        failed_tasks,
        pass_rate,
        total_requests,
        mean_requests_per_task: if total_tasks > 0 {
            (total_requests as f64) / (total_tasks as f64)
        } else {
            0.0
        },
        tokens,
        costs,
        task_latencies,
        request_latencies,
        ttft_quantiles,
        categories,
    }
}
