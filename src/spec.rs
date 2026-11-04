use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SpecError {
    #[error("failed to read specification at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse TOML specification: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("specification validation error: {0}")]
    Validation(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TareSpec {
    pub suite: SuiteConfig,
    #[serde(default)]
    pub budgets: BudgetsConfig,
    #[serde(default)]
    pub gate: GateConfig,
    #[serde(default)]
    pub pricing: BTreeMap<String, ModelPricingConfig>,
    #[serde(default)]
    pub category: BTreeMap<String, CategoryConfig>,
    #[serde(default)]
    pub economics: EconomicsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteConfig {
    pub name: String,
    pub version: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetsConfig {
    pub max_cpst_usd: Option<f64>,
    pub max_cpc_usd: Option<f64>,
    pub max_p50_ms: Option<f64>,
    pub max_p95_ms: Option<f64>,
    pub min_cache_hit_rate: Option<f64>,
    pub max_failure_tax_multiplier: Option<f64>,
}

impl Default for BudgetsConfig {
    fn default() -> Self {
        Self {
            max_cpst_usd: Some(0.10),
            max_cpc_usd: Some(0.05),
            max_p50_ms: Some(1500.0),
            max_p95_ms: Some(3000.0),
            min_cache_hit_rate: Some(0.20),
            max_failure_tax_multiplier: Some(2.5),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateConfig {
    pub min_quality_floor: f64,
    pub max_quality_drop: f64,
    pub tost_equivalence_margin: f64,
    pub mcnemar_alpha: f64,
    pub efficiency_win_min_cpst_cut_pct: f64,
    pub max_cpst_increase_pct: f64,
    pub max_p95_increase_pct: f64,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            min_quality_floor: 0.85,
            max_quality_drop: 0.02,
            tost_equivalence_margin: 0.02,
            mcnemar_alpha: 0.05,
            efficiency_win_min_cpst_cut_pct: 20.0,
            max_cpst_increase_pct: 10.0,
            max_p95_increase_pct: 15.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricingConfig {
    pub input_per_million: f64,
    pub cache_read_per_million: f64,
    pub cache_write_per_million: f64,
    pub output_per_million: f64,
    #[serde(default)]
    pub reasoning_per_million: Option<f64>,
}

impl Default for ModelPricingConfig {
    fn default() -> Self {
        Self {
            input_per_million: 3.00,
            cache_read_per_million: 0.30,
            cache_write_per_million: 3.75,
            output_per_million: 15.00,
            reasoning_per_million: Some(15.00),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryConfig {
    pub baseline_pass_rate: f64,
    pub floor_pass_rate: Option<f64>,
    pub max_drop: Option<f64>,
    pub target_cpst_usd: Option<f64>,
    pub target_p95_ms: Option<f64>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicsConfig {
    pub daily_successful_tasks: usize,
    pub developer_hourly_rate_usd: f64,
    pub latency_seconds_to_dev_cost_ratio: f64,
}

impl Default for EconomicsConfig {
    fn default() -> Self {
        Self {
            daily_successful_tasks: 50000,
            developer_hourly_rate_usd: 150.0,
            latency_seconds_to_dev_cost_ratio: 0.04,
        }
    }
}

impl TareSpec {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, SpecError> {
        let p = path.as_ref();
        let contents = fs::read_to_string(p).map_err(|e| SpecError::Io {
            path: p.display().to_string(),
            source: e,
        })?;
        let spec: Self = toml::from_str(&contents)?;
        spec.validate()?;
        Ok(spec)
    }

    pub fn validate(&self) -> Result<(), SpecError> {
        if self.suite.name.trim().is_empty() {
            return Err(SpecError::Validation(
                "suite.name cannot be empty".to_string(),
            ));
        }
        if self.gate.min_quality_floor < 0.0 || self.gate.min_quality_floor > 1.0 {
            return Err(SpecError::Validation(
                "gate.min_quality_floor must be between 0.0 and 1.0".to_string(),
            ));
        }
        if self.gate.max_quality_drop < 0.0 || self.gate.max_quality_drop > 1.0 {
            return Err(SpecError::Validation(
                "gate.max_quality_drop must be between 0.0 and 1.0".to_string(),
            ));
        }
        for (name, cat) in &self.category {
            if cat.baseline_pass_rate < 0.0 || cat.baseline_pass_rate > 1.0 {
                return Err(SpecError::Validation(format!(
                    "category '{name}' baseline_pass_rate must be between 0.0 and 1.0"
                )));
            }
        }
        Ok(())
    }

    pub fn get_pricing(&self, model: &str) -> ModelPricingConfig {
        if let Some(pricing) = self.pricing.get(model) {
            pricing.clone()
        } else {
            ModelPricingConfig::default()
        }
    }
}
