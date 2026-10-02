use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const LOCKFILE_NAME: &str = ".docgov.lock";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocgovLock {
    pub protocol_version: String,
    pub upstream: LockUpstream,
    #[serde(default)]
    pub artifacts: LockArtifacts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockUpstream {
    pub source: String,
    pub r#ref: String,
    pub synced_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LockArtifacts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_directives: Option<ArtifactEntry>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governance_docs: Option<ArtifactEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactEntry {
    pub target: String,
    pub hash: String, // sha256:...
}

impl DocgovLock {
    pub fn new(protocol_version: &str, source: &str, r#ref: &str) -> Self {
        let now = now_iso8601();
        Self {
            protocol_version: protocol_version.to_string(),
            upstream: LockUpstream {
                source: source.to_string(),
                r#ref: r#ref.to_string(),
                synced_at: now,
            },
            artifacts: LockArtifacts::default(),
        }
    }

    pub fn lockfile_path<P: AsRef<Path>>(root: P) -> PathBuf {
        let r = root.as_ref();
        let dog_lock = r.join(".dog.lock");
        if dog_lock.exists() {
            return dog_lock;
        }
        r.join(LOCKFILE_NAME)
    }

    pub fn load_from_dir<P: AsRef<Path>>(root: P) -> Result<Option<Self>> {
        let path = Self::lockfile_path(root);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)?;
        let lock: Self = serde_yaml::from_str(&content)?;
        Ok(Some(lock))
    }

    pub fn save_to_dir<P: AsRef<Path>>(&self, root: P) -> Result<()> {
        let path = Self::lockfile_path(root);
        let header = "# Generated automatically by docgov. Do not edit manually.\n";
        let yaml = serde_yaml::to_string(self)?;
        fs::write(path, format!("{}{}", header, yaml))?;
        Ok(())
    }
}

pub fn compute_sha256(content: &str) -> String {
    compute_sha256_bytes(content.as_bytes())
}

pub fn compute_sha256_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

/// Canonical ISO-8601 UTC timestamp generator (format: `YYYY-MM-DDTHH:MM:SSZ`).
///
/// Implemented without external date libraries to preserve the zero-vendoring,
/// minimal-dependency posture of the engine.
pub fn now_iso8601() -> String {
    // Simple ISO timestamp format using system time
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // format as YYYY-MM-DDTHH:MM:SSZ
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;

    // Approximate date for lockfile representation
    let mut y = 1970;
    let mut d = days;
    loop {
        let leap = if (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0) {
            1
        } else {
            0
        };
        let days_in_year = 365 + leap;
        if d < days_in_year {
            let month_days = [31, 28 + leap, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
            let mut mo = 1;
            for &md in &month_days {
                if d < md {
                    break;
                }
                d -= md;
                mo += 1;
            }
            let day = d + 1;
            return format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, day, h, m, s);
        }
        d -= days_in_year;
        y += 1;
    }
}
