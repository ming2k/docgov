use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use colored::*;
use std::path::PathBuf;

use crate::config::Config;
use crate::diagnostics::Severity;
use crate::engine::LintEngine;
use crate::git::get_changed_files;
use crate::lockfile::{compute_sha256, ArtifactEntry, DocgovLock};
use crate::remote::RemoteClient;
use crate::rules::lint_05_agent_directives::{
    extract_directives_block, patch_agent_directives, PatchAction,
};

/// The governance specification **major** version this engine can faithfully evaluate.
///
/// This is the engine half of the Spec/Engine Handshake Contract: a repository may
/// pin any spec version whose major equals this value, and `docgov update` will refuse
/// to advance a repository onto a spec major it cannot enforce (which would otherwise
/// yield a false "compliant" verdict).
pub const SUPPORTED_SPEC_MAJOR: u64 = 0;

#[derive(Parser, Debug)]
#[command(
    name = "docgov",
    version,
    about = "High-performance, zero-vendoring documentation and architecture governance linter",
    long_about = "A fast, deterministic compiler-grade linter for Protocol v0.0.1 documentation governance and system invariants."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to target workspace directory (defaults to current directory)
    #[arg(short, long, global = true)]
    pub path: Option<PathBuf>,

    /// Output format
    #[arg(short, long, global = true, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum OutputFormat {
    Text,
    Github,
    Json,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Check repository compliance with all Protocol v0.0.1 invariants
    Check {
        /// Also run git-diff trigger matrix checks
        #[arg(long)]
        diff: bool,

        /// Base git reference to compare against (e.g. 'origin/main', 'HEAD~1')
        #[arg(long)]
        base: Option<String>,

        /// Path to baseline file to suppress known legacy violations
        #[arg(long)]
        baseline: Option<PathBuf>,

        /// Generate a baseline file containing all current diagnostics and exit 0
        #[arg(long)]
        generate_baseline: Option<PathBuf>,
    },

    /// Run only git-diff code-to-doc trigger matrix checks
    Diff {
        /// Base git reference to compare against (e.g. 'origin/main', 'HEAD~1')
        #[arg(default_value = "HEAD~1")]
        base: String,
    },

    /// Automatically fix governance violations (frontmatter synthesis, directives, etc.)
    Fix {
        /// Preview changes without writing them to disk
        #[arg(long)]
        dry_run: bool,

        /// Specific rule IDs to fix (comma-separated, e.g. 'INV-LINT-03,INV-LINT-05')
        #[arg(long, value_delimiter = ',')]
        rules: Option<Vec<String>>,
    },

    /// Initialize standard .docgov.yml configuration and AGENTS.md in the current repository
    Init {
        /// Overwrite existing configuration or refresh directive blocks
        #[arg(long, short = 'F')]
        force: bool,
    },

    /// Sync and atomically update canonical governance documentation and directives
    Sync {
        /// Force re-fetch and re-download assets
        #[arg(long, short = 'F')]
        force: bool,

        /// Synchronize from local specification directory (spec/)
        #[arg(long)]
        local: bool,
    },

    /// Check and upgrade repository governance specification to the latest version
    Update {
        /// Target version to update to (e.g. 'v0.1.1', '0.1.1', or 'latest'). Defaults to latest upstream release.
        #[arg(value_name = "TARGET_REF")]
        target: Option<String>,

        /// Check if a specification update is available without modifying files (exit code 1 if update available)
        #[arg(long)]
        check: bool,

        /// Preview specification upgrade plan without writing changes to disk
        #[arg(long)]
        dry_run: bool,

        /// Force update even if already on the target version or downgrading
        #[arg(long, short = 'F')]
        force: bool,
    },
}

pub fn run() -> Result<i32> {
    let cli = Cli::parse();
    let target_dir = cli.path.unwrap_or_else(|| PathBuf::from("."));

    match cli.command.unwrap_or(Commands::Check {
        diff: false,
        base: None,
        baseline: None,
        generate_baseline: None,
    }) {
        Commands::Check {
            diff,
            base,
            baseline,
            generate_baseline,
        } => {
            let start = std::time::Instant::now();
            let engine = LintEngine::new(&target_dir)?;

            let changed = if diff || base.is_some() {
                Some(get_changed_files(&target_dir, base.as_deref())?)
            } else {
                None
            };

            let raw_diagnostics = engine.run_lint(changed.as_deref())?;

            if let Some(baseline_out) = generate_baseline {
                let base_obj = crate::baseline::Baseline::from_diagnostics(
                    &engine.config.version,
                    &raw_diagnostics,
                );
                let out_path = if baseline_out.is_absolute() {
                    baseline_out
                } else {
                    target_dir.join(baseline_out)
                };
                base_obj.save_to_file(&out_path)?;
                println!(
                    "{} Generated baseline with {} violation(s) at {}",
                    "✔".green().bold(),
                    base_obj.entries.len(),
                    out_path.display()
                );
                return Ok(0);
            }

            let baseline_path = baseline
                .map(|p| {
                    if p.is_absolute() {
                        p
                    } else {
                        target_dir.join(p)
                    }
                })
                .or_else(|| engine.config.baseline.as_ref().map(|p| target_dir.join(p)));

            let (diagnostics, suppressed_count) = if let Some(bpath) = baseline_path {
                if bpath.exists() {
                    let base_obj = crate::baseline::Baseline::load_from_file(&bpath)?;
                    let (filtered, supp) = base_obj.filter_diagnostics(raw_diagnostics);
                    (filtered, supp)
                } else {
                    (raw_diagnostics, 0)
                }
            } else {
                (raw_diagnostics, 0)
            };

            let duration = start.elapsed();

            output_diagnostics(&diagnostics, cli.format, duration, suppressed_count);

            let error_count = diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .count();

            check_for_spec_update_hint(&target_dir, &engine.config);

            if error_count > 0 {
                Ok(1)
            } else {
                Ok(0)
            }
        }
        Commands::Fix { dry_run, rules } => {
            let summary = crate::fix::run_fix(&target_dir, rules.as_deref(), dry_run)?;
            if summary.fixed.is_empty() {
                println!(
                    "{} No fixable violations found. Everything up to date.",
                    "✔".green().bold()
                );
            } else {
                let action_verb = if dry_run { "Would fix" } else { "Fixed" };
                println!(
                    "\n{} {} violation(s):\n",
                    action_verb.green().bold(),
                    summary.fixed.len()
                );
                for item in &summary.fixed {
                    println!(
                        "  {} {} - {}",
                        "✔".green(),
                        item.file_path.display(),
                        item.description
                    );
                }
                println!();
            }
            Ok(0)
        }
        Commands::Diff { base } => {
            let start = std::time::Instant::now();
            let engine = LintEngine::new(&target_dir)?;
            let changed = get_changed_files(&target_dir, Some(&base))?;

            // Run only git trigger rule
            let ctx = crate::rules::LintContext {
                workspace_root: &engine.workspace_root,
                config: &engine.config,
                root_files: &[],
                markdown_docs: &std::collections::HashMap::new(),
                changed_files: Some(&changed),
            };

            let rule = crate::rules::lint_04_trigger::GitSyncTriggerRule;
            use crate::rules::Rule;
            let diagnostics = rule.check(&ctx)?;
            let duration = start.elapsed();

            output_diagnostics(&diagnostics, cli.format, duration, 0);

            let error_count = diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .count();
            if error_count > 0 {
                Ok(1)
            } else {
                Ok(0)
            }
        }
        Commands::Init { force } => {
            init_repo(&target_dir, force)?;
            Ok(0)
        }
        Commands::Sync { force, local } => {
            sync_repo(&target_dir, force, local)?;
            Ok(0)
        }
        Commands::Update {
            target,
            check,
            dry_run,
            force,
        } => update_repo(&target_dir, target.as_deref(), check, dry_run, force),
    }
}

fn output_diagnostics(
    diagnostics: &[crate::diagnostics::Diagnostic],
    format: OutputFormat,
    duration: std::time::Duration,
    suppressed_count: usize,
) {
    match format {
        OutputFormat::Text => {
            if diagnostics.is_empty() {
                if suppressed_count > 0 {
                    println!(
                        "{} All Protocol v0.0.1 documentation invariants verified in {:.3}s. (Suppressed {} known baseline violation(s))",
                        "✔".green().bold(),
                        duration.as_secs_f64(),
                        suppressed_count
                    );
                } else {
                    println!(
                        "{} All Protocol v0.0.1 documentation invariants verified in {:.3}s.",
                        "✔".green().bold(),
                        duration.as_secs_f64()
                    );
                }
                return;
            }

            println!();
            for diag in diagnostics {
                println!("{}", diag.render_terminal());
            }

            let errors = diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .count();
            let warnings = diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Warning)
                .count();

            if suppressed_count > 0 {
                println!(
                    "{} Found {} error(s) and {} warning(s) in {:.3}s. (Check failed, {} suppressed by baseline)\n",
                    "✖".red().bold(),
                    errors,
                    warnings,
                    duration.as_secs_f64(),
                    suppressed_count
                );
            } else {
                println!(
                    "{} Found {} error(s) and {} warning(s) in {:.3}s. (Check failed)\n",
                    "✖".red().bold(),
                    errors,
                    warnings,
                    duration.as_secs_f64()
                );
            }
        }
        OutputFormat::Github => {
            for diag in diagnostics {
                println!("{}", diag.render_github());
            }
        }
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(diagnostics) {
                println!("{}", json);
            }
        }
    }
}

fn init_repo(dir: &std::path::Path, force: bool) -> Result<()> {
    let docgov_yml = dir.join(".docgov.yml");

    let yml_content = r#"version: "0.0.1"

# Remote Upstream & Protocol Distribution
upstream:
  source: "https://github.com/ming2k/docgov-spec"
  ref: "v0.0.1"

# Canonical Governance Documentation Mirror (for Agent Context)
governance_docs:
  install: true
  target_dir: "docs/governance/documentation"

# [INV-LINT-01] Root Location Sanitization
root_sanitization:
  enforce: true
  allowed_markdown:
    - "README.md"
    - "CHANGELOG.md"
    - "CONTRIBUTING.md"
    - "AGENTS.md"
    - "LICENSE.md"
    - "SECURITY.md"

# [INV-LINT-02] Contributor Firewall Bindings
firewall:
  enforce: true
  public_surfaces:
    - "docs/tutorials/**"
    - "docs/how-to/**"
    - "docs/reference/**"
    - "docs/explanation/**"
  internal_surfaces:
    - "docs/dev/**"

# [INV-LINT-03] Architecture & Metadata Profile
architecture:
  enforce: true
  adr_path: "docs/adr"
  require_frontmatter:
    enforce: true
    status_enum: ["draft", "accepted", "superseded", "rejected", "deprecated"]
    mandatory_fields: ["id", "title", "status", "date"]
    tolerant_status: true

# [INV-LINT-05] Agent Directives Binding
agent_directives:
  enforce: true
  targets:
    - "AGENTS.md"

# Baseline configuration (optional: suppress known legacy issues)
# baseline: ".docgov-baseline.json"

# [INV-LINT-04] Code-to-Doc Trigger Bindings
#
# Monitored source paths that require documentation synchronization in the same change.
# `require_update` accepts a single glob or a list (any-of semantics).
triggers:
  - watch: "src/**"
    require_update:
      - "docs/**"
      - "README.md"
    message: "Source code modified; documentation must be synchronized in the same commit."
"#;

    if !docgov_yml.exists() || force {
        std::fs::write(&docgov_yml, yml_content)?;
        println!("{} Created {}", "+".green().bold(), docgov_yml.display());
    } else {
        println!("{} Exists: {}", "~".yellow().bold(), docgov_yml.display());
    }

    sync_repo(dir, force, false)
}

fn sync_repo(dir: &std::path::Path, force: bool, local: bool) -> Result<()> {
    // Load configuration to discover agent directive targets, upstream and governance docs settings
    let (cfg, _) = Config::load_from_dir(dir).unwrap_or((Config::default(), None));

    let (source, r#ref) = if local {
        (".".to_string(), "local".to_string())
    } else {
        (cfg.upstream.source.clone(), cfg.upstream.r#ref.clone())
    };

    // Fetch directives using remote client (checks local spec, local cache, or remote endpoints)
    let remote_client = RemoteClient::new(&source, &r#ref);
    let (directives_content, _source_info) = remote_client.fetch_directives(Some(dir))?;

    let targets = if cfg.agent_directives.targets.is_empty() {
        vec!["AGENTS.md".to_string()]
    } else {
        cfg.agent_directives.targets
    };

    let mut lock = DocgovLock::load_from_dir(dir)?
        .unwrap_or_else(|| DocgovLock::new(&cfg.version, &source, &r#ref));
    lock.protocol_version = cfg.version.clone();
    lock.upstream.source = source.clone();
    lock.upstream.r#ref = r#ref.clone();
    // Refresh the sync timestamp: this lockfile is the result of an active sync pass,
    // and downstream consumers (audits, update staleness heuristics) rely on it being
    // an accurate record of *when* the currently pinned artifacts were materialized.
    lock.upstream.synced_at = crate::lockfile::now_iso8601();

    // 1. Synchronize agent directives (non-invasively, preserving custom guidelines & single #)
    for target in targets {
        let target_path = dir.join(&target);
        let existing = if target_path.exists() {
            Some(std::fs::read_to_string(&target_path)?)
        } else {
            None
        };

        let (new_content, action) =
            patch_agent_directives(existing.as_deref(), &directives_content);

        // Compute hash of the directive block inside the patched content
        if let Some(block) = extract_directives_block(&new_content) {
            let block_hash = compute_sha256(block.trim());
            lock.artifacts.agent_directives = Some(ArtifactEntry {
                target: target.clone(),
                hash: block_hash,
            });
        }

        match action {
            PatchAction::Created => {
                std::fs::write(&target_path, &new_content)?;
                println!(
                    "{} Created {} with docgov directives block",
                    "+".green().bold(),
                    target_path.display()
                );
            }
            PatchAction::Appended => {
                std::fs::write(&target_path, &new_content)?;
                println!(
                    "{} Non-invasively inserted docgov directives block into {}",
                    "+".green().bold(),
                    target_path.display()
                );
            }
            PatchAction::Updated => {
                std::fs::write(&target_path, &new_content)?;
                println!(
                    "{} Updated docgov directives block in {}",
                    "~".yellow().bold(),
                    target_path.display()
                );
            }
            PatchAction::Unchanged => {
                if force {
                    std::fs::write(&target_path, &new_content)?;
                    println!(
                        "{} Refreshed docgov directives block in {}",
                        "~".yellow().bold(),
                        target_path.display()
                    );
                } else {
                    println!(
                        "{} Directives up to date: {}",
                        "✔".green().bold(),
                        target_path.display()
                    );
                }
            }
        }
    }

    // 2. Synchronize canonical governance documentation mirror (full atomic mirror replacement & pruning)
    if cfg.governance_docs.install {
        let archive_hash =
            remote_client.sync_governance_docs(dir, &cfg.governance_docs.target_dir, force)?;
        lock.artifacts.governance_docs = Some(ArtifactEntry {
            target: cfg.governance_docs.target_dir.clone(),
            hash: archive_hash,
        });
    }

    lock.save_to_dir(dir)?;
    println!(
        "{} Updated {}",
        "✔".green().bold(),
        DocgovLock::lockfile_path(dir).display()
    );

    Ok(())
}

pub fn update_repo(
    dir: &std::path::Path,
    target: Option<&str>,
    check: bool,
    dry_run: bool,
    force: bool,
) -> Result<i32> {
    let (cfg, config_path) = Config::load_from_dir(dir).unwrap_or((Config::default(), None));
    let lock = DocgovLock::load_from_dir(dir)?;

    let current_ref = lock
        .as_ref()
        .map(|l| l.upstream.r#ref.clone())
        .unwrap_or_else(|| cfg.upstream.r#ref.clone());
    let current_ver = lock
        .as_ref()
        .map(|l| l.protocol_version.clone())
        .unwrap_or_else(|| cfg.version.clone());

    let (canonical_source, migrated) =
        crate::remote::canonicalize_upstream_source(&cfg.upstream.source);
    let remote_client = RemoteClient::new(&canonical_source, &current_ref);

    if migrated {
        println!(
            "{} Upstream specification source automatically migrated: {} -> {}",
            "ℹ".blue().bold(),
            cfg.upstream.source,
            canonical_source
        );
    }

    println!(
        "{} Resolving upstream specification for {} (current: {})...",
        "🔍".cyan(),
        canonical_source,
        current_ref
    );

    let target_tag = match target {
        Some("latest") => {
            let info = remote_client.fetch_latest_release(Some(dir))?;
            info.tag_name
        }
        Some(t) => {
            if t.starts_with('v') {
                t.to_string()
            } else {
                format!("v{}", t)
            }
        }
        None => {
            let info = remote_client.fetch_latest_release(Some(dir))?;
            info.tag_name
        }
    };

    let target_ver = target_tag.trim_start_matches('v').to_string();

    // ---------------------------------------------------------------------
    // Engine/Spec Handshake Guard
    //
    // The CLI declares the governance protocol major version it can faithfully
    // evaluate. A newer spec *major* means new invariants this engine cannot
    // enforce, which would silently produce a false "compliant" verdict.
    // Therefore we hard-fail and direct the user to upgrade the engine first.
    // ---------------------------------------------------------------------
    let (target_maj, _, _, _) = crate::remote::parse_semver_parts(&target_ver);
    if target_maj > SUPPORTED_SPEC_MAJOR {
        eprintln!(
            "{} Specification {} requires a newer docgov engine.\n\
             \x20  Current CLI : v{} (supports spec major {})\n\
             \x20  Requested   : {}\n\
             \x20  Action      : upgrade the CLI first — 'docgov self update' or your package manager.",
            "error[COMPAT]:".red().bold(),
            target_tag,
            env!("CARGO_PKG_VERSION"),
            SUPPORTED_SPEC_MAJOR,
            target_tag
        );
        return Ok(1);
    }

    let cmp = crate::remote::compare_semver(&target_ver, &current_ver);

    match cmp {
        std::cmp::Ordering::Equal => {
            if !force && !migrated {
                if check {
                    println!(
                        "{} Upstream specification is up to date: {}",
                        "✔".green().bold(),
                        current_ref
                    );
                    return Ok(0);
                }
                println!(
                    "{} Repository specification is already up to date: {} ({})",
                    "✔".green().bold(),
                    current_ref,
                    canonical_source
                );
                return Ok(0);
            }
        }
        std::cmp::Ordering::Less => {
            if !force && !migrated {
                if check {
                    println!(
                        "{} Local specification is ahead of target: {} > {}",
                        "✔".green().bold(),
                        current_ref,
                        target_tag
                    );
                    return Ok(0);
                }
                println!(
                    "{} Repository specification ({}) is newer than target ({}) (use --force to downgrade)",
                    "~".yellow().bold(),
                    current_ref,
                    target_tag
                );
                return Ok(0);
            }
        }
        std::cmp::Ordering::Greater => {}
    }

    // New version detected or forced
    if check {
        println!(
            "{} New specification version available: {} -> {} (Run 'docgov update' to upgrade)",
            "ℹ".blue().bold(),
            current_ref,
            target_tag
        );
        return Ok(1); // Standard check mode: return non-zero exit code when updates are available
    }

    if dry_run {
        println!();
        println!(
            "{} Specification Upgrade Preview (dry run):",
            "~".yellow().bold()
        );
        println!("   Upstream: {}", canonical_source);
        if migrated {
            println!("   (Migrated from legacy source: {})", cfg.upstream.source);
        }
        println!("   Current : {} (protocol: {})", current_ref, current_ver);
        println!("   Target  : {} (protocol: {})", target_tag, target_ver);
        println!("   Action  : Would atomically rewrite .docgov.yml and refresh lockfile");
        return Ok(0);
    }

    println!();
    println!(
        "{} Upgrading specification: {} -> {}",
        "🚀".bold(),
        current_ref.yellow(),
        target_tag.green().bold()
    );

    // 1. Non-invasively patch .docgov.yml with atomic rollback protection
    let docgov_yml_path = config_path.unwrap_or_else(|| dir.join(".docgov.yml"));
    let original = if docgov_yml_path.exists() {
        Some(std::fs::read_to_string(&docgov_yml_path)?)
    } else {
        None
    };

    if let Some(ref orig) = original {
        let new_source = if migrated {
            Some(canonical_source.as_str())
        } else {
            None
        };
        let patched = patch_yaml_version_ref_and_source(orig, &target_ver, &target_tag, new_source);
        std::fs::write(&docgov_yml_path, patched)?;
        println!(
            "{} Updated specification declaration in {}",
            "~".yellow().bold(),
            docgov_yml_path.display()
        );
    }

    // 2. Perform synchronized asset pull and lockfile renewal with rollback on failure
    if let Err(err) = sync_repo(dir, true, false) {
        if let Some(ref orig) = original {
            let _ = std::fs::write(&docgov_yml_path, orig);
        }
        return Err(err);
    }

    println!();
    println!(
        "{} Successfully updated specification to {}!",
        "✨".green().bold(),
        target_tag.green().bold()
    );
    println!(
        "{} Run '{}' to verify repository compliance under the new specification.",
        "💡".cyan(),
        "docgov check".bold()
    );

    Ok(0)
}

pub fn patch_yaml_version_and_ref(raw: &str, new_ver: &str, new_ref: &str) -> String {
    patch_yaml_version_ref_and_source(raw, new_ver, new_ref, None)
}

pub fn patch_yaml_version_ref_and_source(
    raw: &str,
    new_ver: &str,
    new_ref: &str,
    new_source: Option<&str>,
) -> String {
    let mut lines = Vec::new();
    let mut in_upstream = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("version:") && !line.starts_with(' ') && !line.starts_with('\t') {
            lines.push(format!("version: \"{}\"", new_ver));
        } else if trimmed == "upstream:" {
            in_upstream = true;
            lines.push(line.to_string());
        } else if in_upstream && (trimmed.starts_with("ref:") || trimmed.starts_with("ref :")) {
            let indent = line.len() - line.trim_start().len();
            let whitespace = &line[..indent];
            lines.push(format!("{}ref: \"{}\"", whitespace, new_ref));
            in_upstream = false;
        } else if in_upstream && (trimmed.starts_with("source:") || trimmed.starts_with("source :"))
        {
            if let Some(src) = new_source {
                let indent = line.len() - line.trim_start().len();
                let whitespace = &line[..indent];
                lines.push(format!("{}source: \"{}\"", whitespace, src));
            } else {
                lines.push(line.to_string());
            }
        } else {
            if in_upstream
                && !trimmed.starts_with('#')
                && !trimmed.starts_with("source:")
                && !trimmed.is_empty()
            {
                in_upstream = false;
            }
            lines.push(line.to_string());
        }
    }

    let mut result = lines.join("\n");
    if raw.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn check_for_spec_update_hint(dir: &std::path::Path, cfg: &Config) {
    if std::env::var("CI").is_ok() {
        return;
    }
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        return;
    }

    let lock = DocgovLock::load_from_dir(dir).ok().flatten();
    let current_ref = lock
        .as_ref()
        .map(|l| l.upstream.r#ref.as_str())
        .unwrap_or(cfg.upstream.r#ref.as_str());

    let client = RemoteClient::new(&cfg.upstream.source, current_ref);
    // 6-hour TTL cache: guarantees `docgov check` never hammers the network on every run.
    const UPDATE_HINT_TTL_SECS: u64 = 6 * 60 * 60;
    if let Ok(info) = client.fetch_latest_release_cached(Some(dir), UPDATE_HINT_TTL_SECS) {
        let current_ver = current_ref.trim_start_matches('v');
        let target_ver = info.tag_name.trim_start_matches('v');
        if crate::remote::compare_semver(target_ver, current_ver) == std::cmp::Ordering::Greater {
            println!();
            println!(
                "{} A new governance specification is available: {} -> {} (Run 'docgov update' to upgrade)",
                "💡".cyan().bold(),
                current_ref.yellow(),
                info.tag_name.green().bold()
            );
        }
    }
}
