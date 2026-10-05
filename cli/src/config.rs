use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_version")]
    pub version: String,

    #[serde(default)]
    pub root_sanitization: RootSanitizationConfig,

    #[serde(default)]
    pub firewall: FirewallConfig,

    #[serde(default)]
    pub architecture: ArchitectureConfig,

    #[serde(default)]
    pub agent_directives: AgentDirectivesConfig,

    #[serde(default)]
    pub upstream: UpstreamConfig,

    #[serde(default)]
    pub governance_docs: GovernanceDocsConfig,

    #[serde(default)]
    pub triggers: Vec<TriggerConfig>,

    #[serde(default)]
    pub baseline: Option<String>,
}

fn default_version() -> String {
    "0.0.2".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: default_version(),
            root_sanitization: RootSanitizationConfig::default(),
            firewall: FirewallConfig::default(),
            architecture: ArchitectureConfig::default(),
            agent_directives: AgentDirectivesConfig::default(),
            upstream: UpstreamConfig::default(),
            governance_docs: GovernanceDocsConfig::default(),
            triggers: Vec::new(),
            baseline: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootSanitizationConfig {
    #[serde(default = "default_true")]
    pub enforce: bool,

    #[serde(default = "default_allowed_markdown")]
    pub allowed_markdown: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_allowed_markdown() -> Vec<String> {
    vec![
        "README.md".to_string(),
        "CHANGELOG.md".to_string(),
        "CONTRIBUTING.md".to_string(),
        "AGENTS.md".to_string(),
        "LICENSE.md".to_string(),
        "SECURITY.md".to_string(),
    ]
}

impl Default for RootSanitizationConfig {
    fn default() -> Self {
        Self {
            enforce: true,
            allowed_markdown: default_allowed_markdown(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallConfig {
    #[serde(default = "default_true")]
    pub enforce: bool,

    #[serde(default = "default_public_surfaces")]
    pub public_surfaces: Vec<String>,

    #[serde(default = "default_internal_surfaces")]
    pub internal_surfaces: Vec<String>,
}

fn default_public_surfaces() -> Vec<String> {
    vec![
        "docs/tutorials/**".to_string(),
        "docs/how-to/**".to_string(),
        "docs/reference/**".to_string(),
        "docs/explanation/**".to_string(),
    ]
}

fn default_internal_surfaces() -> Vec<String> {
    vec!["docs/dev/**".to_string()]
}

impl Default for FirewallConfig {
    fn default() -> Self {
        Self {
            enforce: true,
            public_surfaces: default_public_surfaces(),
            internal_surfaces: default_internal_surfaces(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureConfig {
    #[serde(default = "default_true")]
    pub enforce: bool,

    #[serde(default = "default_adr_path")]
    pub adr_path: String,

    #[serde(default)]
    pub require_frontmatter: RequireFrontmatterConfig,
}

fn default_adr_path() -> String {
    "docs/adr".to_string()
}

impl Default for ArchitectureConfig {
    fn default() -> Self {
        Self {
            enforce: true,
            adr_path: default_adr_path(),
            require_frontmatter: RequireFrontmatterConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequireFrontmatterConfig {
    #[serde(default = "default_true")]
    pub enforce: bool,

    #[serde(default)]
    pub exclude: Vec<String>,

    #[serde(default = "default_status_enum")]
    pub status_enum: Vec<String>,

    #[serde(default = "default_mandatory_fields")]
    pub mandatory_fields: Vec<String>,

    #[serde(default = "default_true")]
    pub tolerant_status: bool,
}

fn default_status_enum() -> Vec<String> {
    vec![
        "draft".to_string(),
        "accepted".to_string(),
        "superseded".to_string(),
        "rejected".to_string(),
        "deprecated".to_string(),
    ]
}

fn default_mandatory_fields() -> Vec<String> {
    vec![
        "id".to_string(),
        "title".to_string(),
        "status".to_string(),
        "date".to_string(),
    ]
}

impl Default for RequireFrontmatterConfig {
    fn default() -> Self {
        Self {
            enforce: true,
            exclude: Vec::new(),
            status_enum: default_status_enum(),
            mandatory_fields: default_mandatory_fields(),
            tolerant_status: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDirectivesConfig {
    #[serde(default = "default_true")]
    pub enforce: bool,

    #[serde(default = "default_agent_targets")]
    pub targets: Vec<String>,
}

fn default_agent_targets() -> Vec<String> {
    vec!["AGENTS.md".to_string()]
}

impl Default for AgentDirectivesConfig {
    fn default() -> Self {
        Self {
            enforce: true,
            targets: default_agent_targets(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamConfig {
    #[serde(default = "default_upstream_source")]
    pub source: String,

    #[serde(default = "default_upstream_ref")]
    pub r#ref: String,
}

fn default_upstream_source() -> String {
    "https://github.com/ming2k/docgov".to_string()
}

fn default_upstream_ref() -> String {
    "v0.0.2".to_string()
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            source: default_upstream_source(),
            r#ref: default_upstream_ref(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceDocsConfig {
    #[serde(default = "default_true")]
    pub install: bool,

    #[serde(default = "default_governance_target_dir")]
    pub target_dir: String,
}

fn default_governance_target_dir() -> String {
    "docs/governance/documentation".to_string()
}

impl Default for GovernanceDocsConfig {
    fn default() -> Self {
        Self {
            install: true,
            target_dir: default_governance_target_dir(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    /// Glob of monitored source paths that trigger the requirement.
    pub watch: String,

    /// Documentation surface that must be touched in the same change.
    ///
    /// Accepts either a single glob string or a list of globs. A list expresses
    /// **any-of** semantics: satisfying any one of the globs clears the trigger.
    /// This prevents multiple same-`watch` triggers from silently compounding
    /// into an unsatisfiable AND-constraint.
    #[serde(deserialize_with = "deserialize_string_or_vec")]
    pub require_update: Vec<String>,

    #[serde(default)]
    pub message: Option<String>,
}

fn deserialize_string_or_vec<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        One(String),
        Many(Vec<String>),
    }

    Ok(match StringOrVec::deserialize(deserializer)? {
        StringOrVec::One(s) => vec![s],
        StringOrVec::Many(v) => v,
    })
}

impl Config {
    /// Load configuration from a specified path, or search the root directory
    pub fn load_from_dir<P: AsRef<Path>>(dir: P) -> Result<(Self, Option<PathBuf>)> {
        let dir = dir.as_ref();
        let candidates = [
            dir.join(".dog.yml"),
            dir.join(".dog.yaml"),
            dir.join(".docgov.yml"),
            dir.join(".docgov.yaml"),
            dir.join(".governance.yml"),
            dir.join(".governance.yaml"),
        ];

        for candidate in &candidates {
            if candidate.exists() && candidate.is_file() {
                let content = std::fs::read_to_string(candidate)
                    .with_context(|| format!("Failed to read config file at {:?}", candidate))?;
                let config: Config = serde_yaml::from_str(&content)
                    .with_context(|| format!("Failed to parse YAML in {:?}", candidate))?;
                return Ok((config, Some(candidate.clone())));
            }
        }

        // Return default config if no file found
        Ok((Config::default(), None))
    }
}
