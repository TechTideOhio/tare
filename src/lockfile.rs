use crate::spec::TareSpec;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LockfileError {
    #[error("failed to read file {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse lockfile JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("lockfile drift detected: {0}")]
    Drift(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryLockEntry {
    pub baseline_pass_rate: f64,
    pub floor_pass_rate: Option<f64>,
    pub max_drop: Option<f64>,
    pub target_cpst_usd: Option<f64>,
    pub target_p95_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TareLockfile {
    pub lock_version: String,
    pub suite_name: String,
    pub suite_version: String,
    pub spec_sha256: String,
    pub max_cpst_usd: Option<f64>,
    pub max_p95_ms: Option<f64>,
    pub categories: BTreeMap<String, CategoryLockEntry>,
    pub overall_lock_digest: String,
}

pub fn hash_file_framed(path: impl AsRef<Path>) -> Result<String, LockfileError> {
    let p = path.as_ref();
    let bytes = fs::read(p).map_err(|e| LockfileError::Io {
        path: p.display().to_string(),
        source: e,
    })?;
    let mut hasher = Sha256::new();
    hasher.update(b"sha256-framed-v1\n");
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

impl TareLockfile {
    pub fn create(spec_path: impl AsRef<Path>, spec: &TareSpec) -> Result<Self, LockfileError> {
        let spec_sha = hash_file_framed(spec_path)?;

        let mut cat_map = BTreeMap::new();
        for (cat_name, cat_cfg) in &spec.category {
            cat_map.insert(
                cat_name.clone(),
                CategoryLockEntry {
                    baseline_pass_rate: cat_cfg.baseline_pass_rate,
                    floor_pass_rate: cat_cfg.floor_pass_rate,
                    max_drop: cat_cfg.max_drop,
                    target_cpst_usd: cat_cfg.target_cpst_usd,
                    target_p95_ms: cat_cfg.target_p95_ms,
                },
            );
        }

        let mut overall_hasher = Sha256::new();
        overall_hasher.update(b"sha256-framed-v1\n");
        overall_hasher.update(spec_sha.as_bytes());
        overall_hasher.update(b":");
        let serialized_cats = serde_json::to_string(&cat_map).unwrap_or_default();
        overall_hasher.update(serialized_cats.as_bytes());
        let overall_digest = format!("{:x}", overall_hasher.finalize());

        Ok(Self {
            lock_version: "sha256-framed-v1".to_string(),
            suite_name: spec.suite.name.clone(),
            suite_version: spec.suite.version.clone(),
            spec_sha256: spec_sha,
            max_cpst_usd: spec.budgets.max_cpst_usd,
            max_p95_ms: spec.budgets.max_p95_ms,
            categories: cat_map,
            overall_lock_digest: overall_digest,
        })
    }

    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), LockfileError> {
        let p = path.as_ref();
        let json = serde_json::to_string_pretty(self)?;
        fs::write(p, json).map_err(|e| LockfileError::Io {
            path: p.display().to_string(),
            source: e,
        })?;
        Ok(())
    }

    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, LockfileError> {
        let p = path.as_ref();
        let bytes = fs::read(p).map_err(|e| LockfileError::Io {
            path: p.display().to_string(),
            source: e,
        })?;
        let lockfile: Self = serde_json::from_slice(&bytes)?;
        Ok(lockfile)
    }

    pub fn verify(
        &self,
        spec_path: impl AsRef<Path>,
        spec: &TareSpec,
    ) -> Result<(), LockfileError> {
        let current_spec_sha = hash_file_framed(spec_path)?;
        if current_spec_sha != self.spec_sha256 {
            return Err(LockfileError::Drift(format!(
                "specification SHA-256 drift: locked {}, current {}",
                self.spec_sha256, current_spec_sha
            )));
        }

        for (cat_name, locked) in &self.categories {
            if let Some(current_cat) = spec.category.get(cat_name) {
                if (current_cat.baseline_pass_rate - locked.baseline_pass_rate).abs() > 1e-6 {
                    return Err(LockfileError::Drift(format!(
                        "category '{cat_name}' baseline pass rate drift: locked {}, spec {}",
                        locked.baseline_pass_rate, current_cat.baseline_pass_rate
                    )));
                }
            } else {
                return Err(LockfileError::Drift(format!(
                    "category '{cat_name}' found in lockfile but missing in specification"
                )));
            }
        }
        Ok(())
    }
}
