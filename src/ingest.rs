use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IngestError {
    #[error("failed to read trace file {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid JSON at line {line}: {message}")]
    Json { line: usize, message: String },
    #[error("dataset contains no valid task records")]
    EmptyDataset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestMetrics {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub cached_input_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub reasoning_tokens: u64,
    #[serde(default)]
    pub duration_ms: f64,
    #[serde(default)]
    pub ttft_ms: Option<f64>,
    #[serde(default)]
    pub cost_usd_micros: Option<u64>,
}

impl RequestMetrics {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.cached_input_tokens)
            .saturating_add(self.cache_write_tokens)
            .saturating_add(self.output_tokens)
            .saturating_add(self.reasoning_tokens)
    }

    pub fn total_input_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.cached_input_tokens)
            .saturating_add(self.cache_write_tokens)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRunRecord {
    pub task_id: String,
    #[serde(default = "default_category")]
    pub category: String,
    pub passed: bool,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub requests: Vec<RequestMetrics>,
    #[serde(default)]
    pub wall_clock_ms: Option<f64>,
    #[serde(default)]
    pub retries: usize,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

fn default_category() -> String {
    "general".to_string()
}

fn default_model() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, Deserialize)]
struct RawLineRecord {
    pub task_id: Option<String>,
    pub id: Option<String>,
    pub category: Option<String>,
    pub passed: Option<bool>,
    pub success: Option<bool>,
    pub outcome: Option<String>,
    pub model: Option<String>,
    pub requests: Option<Vec<RequestMetrics>>,
    pub wall_clock_ms: Option<f64>,
    pub retries: Option<usize>,
    pub input_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
    pub duration_ms: Option<f64>,
    pub ttft_ms: Option<f64>,
    pub cost_usd_micros: Option<u64>,
}

pub fn load_task_runs(path: impl AsRef<Path>) -> Result<Vec<TaskRunRecord>, IngestError> {
    let p = path.as_ref();
    let file = File::open(p).map_err(|e| IngestError::Io {
        path: p.display().to_string(),
        source: e,
    })?;
    let reader = BufReader::new(file);

    let mut tasks = Vec::new();
    let mut line_num = 0;

    for line_result in reader.lines() {
        line_num += 1;
        let line = line_result.map_err(|e| IngestError::Io {
            path: p.display().to_string(),
            source: e,
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let raw: RawLineRecord = serde_json::from_str(trimmed).map_err(|e| IngestError::Json {
            line: line_num,
            message: e.to_string(),
        })?;

        let task_id = raw
            .task_id
            .or(raw.id)
            .unwrap_or_else(|| format!("task_{line_num}"));
        let category = raw.category.unwrap_or_else(default_category);
        let passed = raw
            .passed
            .or(raw.success)
            .or_else(|| raw.outcome.as_ref().map(|o| o == "ok" || o == "pass"))
            .unwrap_or(false);
        let model = raw.model.unwrap_or_else(default_model);

        let requests = if let Some(reqs) = raw.requests {
            reqs
        } else {
            vec![RequestMetrics {
                input_tokens: raw.input_tokens.unwrap_or(0),
                cached_input_tokens: raw.cached_input_tokens.unwrap_or(0),
                cache_write_tokens: raw.cache_write_tokens.unwrap_or(0),
                output_tokens: raw.output_tokens.unwrap_or(0),
                reasoning_tokens: raw.reasoning_tokens.unwrap_or(0),
                duration_ms: raw.duration_ms.unwrap_or(0.0),
                ttft_ms: raw.ttft_ms,
                cost_usd_micros: raw.cost_usd_micros,
            }]
        };

        tasks.push(TaskRunRecord {
            task_id,
            category,
            passed,
            model,
            requests,
            wall_clock_ms: raw.wall_clock_ms,
            retries: raw.retries.unwrap_or(0),
            metadata: BTreeMap::new(),
        });
    }

    if tasks.is_empty() {
        return Err(IngestError::EmptyDataset);
    }

    Ok(tasks)
}

pub fn write_task_runs(path: impl AsRef<Path>, tasks: &[TaskRunRecord]) -> Result<(), IngestError> {
    let p = path.as_ref();
    let mut file = File::create(p).map_err(|e| IngestError::Io {
        path: p.display().to_string(),
        source: e,
    })?;

    for task in tasks {
        let json = serde_json::to_string(task).map_err(|e| IngestError::Json {
            line: 0,
            message: e.to_string(),
        })?;
        writeln!(file, "{json}").map_err(|e| IngestError::Io {
            path: p.display().to_string(),
            source: e,
        })?;
    }
    Ok(())
}
