use crate::metrics::RunProfile;
use crate::spec::TareSpec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionEconomics {
    pub daily_successful_tasks: usize,
    pub baseline_cpst_usd: f64,
    pub candidate_cpst_usd: f64,
    pub cpst_savings_per_task_usd: f64,
    pub daily_api_savings_usd: f64,
    pub annual_api_savings_usd: f64,
    pub baseline_failure_waste_rate: f64,
    pub candidate_failure_waste_rate: f64,
    pub annual_failure_waste_savings_usd: f64,
    pub p95_latency_reduction_seconds: f64,
    pub annual_latency_productivity_value_usd: f64,
    pub total_annual_economic_impact_usd: f64,
}

impl ProductionEconomics {
    pub fn calculate(spec: &TareSpec, baseline: &RunProfile, candidate: &RunProfile) -> Self {
        let daily_tasks = spec.economics.daily_successful_tasks;
        let b_cpst = baseline.costs.cpst_usd.unwrap_or(0.0);
        let c_cpst = candidate.costs.cpst_usd.unwrap_or(0.0);
        let cpst_savings_per_task = (b_cpst - c_cpst).max(0.0);

        let daily_api_savings = (daily_tasks as f64) * cpst_savings_per_task;
        let annual_api_savings = daily_api_savings * 365.0;

        let b_waste_rate = if baseline.costs.total_cost_usd > 0.0 {
            baseline.costs.failure_waste_usd / baseline.costs.total_cost_usd
        } else {
            0.0
        };
        let c_waste_rate = if candidate.costs.total_cost_usd > 0.0 {
            candidate.costs.failure_waste_usd / candidate.costs.total_cost_usd
        } else {
            0.0
        };

        let baseline_annual_spend = (daily_tasks as f64) * b_cpst * 365.0;
        let annual_failure_waste_savings =
            ((b_waste_rate - c_waste_rate).max(0.0)) * baseline_annual_spend;

        let p95_reduction_ms =
            (baseline.task_latencies.p95_ms - candidate.task_latencies.p95_ms).max(0.0);
        let p95_reduction_sec = p95_reduction_ms / 1000.0;

        let total_saved_hours_daily = (p95_reduction_sec * (daily_tasks as f64)) / 3600.0;
        let annual_latency_value = total_saved_hours_daily
            * 365.0
            * spec.economics.developer_hourly_rate_usd
            * spec.economics.latency_seconds_to_dev_cost_ratio;

        let total_annual_economic_impact =
            annual_api_savings + annual_failure_waste_savings + annual_latency_value;

        Self {
            daily_successful_tasks: daily_tasks,
            baseline_cpst_usd: b_cpst,
            candidate_cpst_usd: c_cpst,
            cpst_savings_per_task_usd: cpst_savings_per_task,
            daily_api_savings_usd: daily_api_savings,
            annual_api_savings_usd: annual_api_savings,
            baseline_failure_waste_rate: b_waste_rate,
            candidate_failure_waste_rate: c_waste_rate,
            annual_failure_waste_savings_usd: annual_failure_waste_savings,
            p95_latency_reduction_seconds: p95_reduction_sec,
            annual_latency_productivity_value_usd: annual_latency_value,
            total_annual_economic_impact_usd: total_annual_economic_impact,
        }
    }
}
