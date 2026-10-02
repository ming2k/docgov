use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::diagnostics::Diagnostic;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BaselineEntry {
    pub rule_id: String,
    pub file_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    pub version: String,
    pub entries: Vec<BaselineEntry>,
}

impl Baseline {
    pub fn new(version: &str) -> Self {
        Self {
            version: version.to_string(),
            entries: Vec::new(),
        }
    }

    pub fn from_diagnostics(version: &str, diagnostics: &[Diagnostic]) -> Self {
        let mut entries = diagnostics
            .iter()
            .map(|d| BaselineEntry {
                rule_id: d.rule_id.clone(),
                file_path: d.file_path.to_string_lossy().replace('\\', "/"),
                message: d.message.clone(),
            })
            .collect::<Vec<_>>();

        entries.sort_by(|a, b| {
            a.rule_id
                .cmp(&b.rule_id)
                .then(a.file_path.cmp(&b.file_path))
                .then(a.message.cmp(&b.message))
        });
        entries.dedup();

        Self {
            version: version.to_string(),
            entries,
        }
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref()).with_context(|| {
            format!(
                "Failed to read baseline file at {}",
                path.as_ref().display()
            )
        })?;
        let baseline: Baseline = serde_json::from_str(&content).with_context(|| {
            format!(
                "Failed to parse baseline file at {}",
                path.as_ref().display()
            )
        })?;
        Ok(baseline)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path.as_ref(), json).with_context(|| {
            format!(
                "Failed to write baseline file at {}",
                path.as_ref().display()
            )
        })?;
        Ok(())
    }

    pub fn filter_diagnostics(&self, diagnostics: Vec<Diagnostic>) -> (Vec<Diagnostic>, usize) {
        let mut remaining = Vec::new();
        let mut suppressed = 0;

        for diag in diagnostics {
            let normalized_path = diag.file_path.to_string_lossy().replace('\\', "/");
            let matched = self.entries.iter().any(|entry| {
                entry.rule_id == diag.rule_id
                    && (entry.file_path == normalized_path
                        || normalized_path.ends_with(&entry.file_path)
                        || entry.file_path.ends_with(&normalized_path))
                    && entry.message == diag.message
            });

            if matched {
                suppressed += 1;
            } else {
                remaining.push(diag);
            }
        }

        (remaining, suppressed)
    }
}
