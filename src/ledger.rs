use crate::ingest::TaskRunRecord;
use crate::metrics::compute_task_duration_ms;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("failed to access ledger at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to serialize or deserialize ledger entry: {0}")]
    Json(#[from] serde_json::Error),
    #[error(
        "ledger integrity breach at index {index}: expected prev_hash {expected}, found {found}"
    )]
    IntegrityBreach {
        index: usize,
        expected: String,
        found: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLedgerEntry {
    pub index: usize,
    pub task_id: String,
    pub category: String,
    pub passed: bool,
    pub model: String,
    pub total_tokens: u64,
    pub duration_ms: f64,
    pub cost_usd_micros: u64,
    pub prev_entry_hash: String,
    pub entry_hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TareLedger {
    pub version: String,
    pub total_entries: usize,
    pub head_hash: String,
    pub entries: Vec<TaskLedgerEntry>,
}

pub fn hash_task_entry(
    index: usize,
    task_id: &str,
    category: &str,
    passed: bool,
    model: &str,
    total_tokens: u64,
    duration_ms: f64,
    cost_usd_micros: u64,
    prev_hash: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"tare-run-v1\n");
    hasher.update(index.to_le_bytes());
    hasher.update(task_id.as_bytes());
    hasher.update(b":");
    hasher.update(category.as_bytes());
    hasher.update(b":");
    hasher.update(if passed { b"1" } else { b"0" });
    hasher.update(b":");
    hasher.update(model.as_bytes());
    hasher.update(b":");
    hasher.update(total_tokens.to_le_bytes());
    hasher.update(b":");
    hasher.update(duration_ms.to_bits().to_le_bytes());
    hasher.update(b":");
    hasher.update(cost_usd_micros.to_le_bytes());
    hasher.update(b":");
    hasher.update(prev_hash.as_bytes());
    format!("{:x}", hasher.finalize())
}

impl TareLedger {
    pub fn create_from_tasks(tasks: &[TaskRunRecord], spec: &crate::spec::TareSpec) -> Self {
        let mut entries = Vec::with_capacity(tasks.len());
        let mut prev_hash =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();

        for (idx, task) in tasks.iter().enumerate() {
            let pricing = spec.get_pricing(&task.model);
            let cost_f64 = crate::metrics::compute_task_cost(task, &pricing);
            let cost_micros = (cost_f64 * 1_000_000.0).round() as u64;
            let total_tokens: u64 = task.requests.iter().map(|r| r.total_tokens()).sum();
            let duration_ms = compute_task_duration_ms(task);

            let entry_hash = hash_task_entry(
                idx,
                &task.task_id,
                &task.category,
                task.passed,
                &task.model,
                total_tokens,
                duration_ms,
                cost_micros,
                &prev_hash,
            );

            entries.push(TaskLedgerEntry {
                index: idx,
                task_id: task.task_id.clone(),
                category: task.category.clone(),
                passed: task.passed,
                model: task.model.clone(),
                total_tokens,
                duration_ms,
                cost_usd_micros: cost_micros,
                prev_entry_hash: prev_hash.clone(),
                entry_hash: entry_hash.clone(),
            });

            prev_hash = entry_hash;
        }

        Self {
            version: "tare-run-v1".to_string(),
            total_entries: entries.len(),
            head_hash: prev_hash,
            entries,
        }
    }

    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), LedgerError> {
        let p = path.as_ref();
        let mut file = File::create(p).map_err(|e| LedgerError::Io {
            path: p.display().to_string(),
            source: e,
        })?;

        for entry in &self.entries {
            let json = serde_json::to_string(entry)?;
            writeln!(file, "{json}").map_err(|e| LedgerError::Io {
                path: p.display().to_string(),
                source: e,
            })?;
        }
        Ok(())
    }

    pub fn load_and_verify(path: impl AsRef<Path>) -> Result<Self, LedgerError> {
        let p = path.as_ref();
        let file = File::open(p).map_err(|e| LedgerError::Io {
            path: p.display().to_string(),
            source: e,
        })?;
        let reader = BufReader::new(file);

        let mut entries = Vec::new();
        let mut expected_prev =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();

        for (idx, line_res) in reader.lines().enumerate() {
            let line = line_res.map_err(|e| LedgerError::Io {
                path: p.display().to_string(),
                source: e,
            })?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let entry: TaskLedgerEntry = serde_json::from_str(trimmed)?;

            if entry.prev_entry_hash != expected_prev {
                return Err(LedgerError::IntegrityBreach {
                    index: idx,
                    expected: expected_prev,
                    found: entry.prev_entry_hash,
                });
            }

            let computed_hash = hash_task_entry(
                entry.index,
                &entry.task_id,
                &entry.category,
                entry.passed,
                &entry.model,
                entry.total_tokens,
                entry.duration_ms,
                entry.cost_usd_micros,
                &entry.prev_entry_hash,
            );

            if computed_hash != entry.entry_hash {
                return Err(LedgerError::IntegrityBreach {
                    index: idx,
                    expected: computed_hash,
                    found: entry.entry_hash,
                });
            }

            expected_prev = entry.entry_hash.clone();
            entries.push(entry);
        }

        Ok(Self {
            version: "tare-run-v1".to_string(),
            total_entries: entries.len(),
            head_hash: expected_prev,
            entries,
        })
    }
}
