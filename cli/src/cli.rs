use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::config::Config;
use crate::diagnostics::{
    Diagnostic, DiagnosticReport, ReportSummary, Severity, JSON_SCHEMA_VERSION,
};
use crate::engine::LintEngine;
use crate::git::get_changed_files;
use crate::lockfile::{compute_sha256, ArtifactEntry, DocgovLock};
use crate::output::{self, ColorChoice, Sym};
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

/// The compiled engine version, single-sourced from `Cargo.toml`.
///
/// This is exposed as a machine-readable, bare semantic version string (e.g. `0.0.4`)
/// so release tooling and scripts can parse stdout directly without stripping a
/// program-name prefix.
pub const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(
    name = "docgov",
    disable_version_flag = true,
    about = "High-performance, zero-vendoring documentation and architecture governance linter",
    long_about = "A fast, deterministic compiler-grade linter for documentation governance and system invariants."
)]
pub struct Cli {
    /// Print the engine version and exit
    #[arg(short = 'V', long = "version", global = true)]
    pub version: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to target workspace directory (defaults to current directory)
    #[arg(short, long, global = true)]
    pub path: Option<PathBuf>,

    /// Output format
    #[arg(short, long, global = true, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// When to colorize human-readable output
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
}

impl ValueEnum for ColorChoice {
    fn value_variants<'a>() -> &'a [Self] {
        &[ColorChoice::Auto, ColorChoice::Always, ColorChoice::Never]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            ColorChoice::Auto => clap::builder::PossibleValue::new("auto")
                .help("Colorize only when stdout is a terminal"),
            ColorChoice::Always => {
                clap::builder::PossibleValue::new("always").help("Always colorize output")
            }
            ColorChoice::Never => {
                clap::builder::PossibleValue::new("never").help("Never colorize output")
            }
        })
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum OutputFormat {
    Text,
    Github,
    Json,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Check repository compliance with all Protocol invariants
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

    /// Initialize standard .docgov.yml configuration and AGENTS.md in the current repository
    Init {
        /// Overwrite existing configuration or refresh directive blocks
        #[arg(long, short = 'F')]
        force: bool,
    },

    /// Update specification, synchronize remote assets, and refresh governance lockfile
    Update {
        /// Target version to update to (e.g. 'v0.1.1', '0.1.1', or 'latest'). Defaults to current upstream ref or latest release.
        #[arg(value_name = "TARGET_REF")]
        target: Option<String>,

        /// Check if a specification update is available without modifying files (exit code 1 if update available)
        #[arg(long)]
        check: bool,

        /// Preview specification upgrade plan without writing changes to disk
        #[arg(long)]
        dry_run: bool,

        /// Force re-fetch and re-synchronize assets even if version unchanged or downgrading
        #[arg(long, short = 'F')]
        force: bool,

        /// Synchronize from local specification directory (spec/)
        #[arg(long)]
        local: bool,
    },
}

/// Process entrypoint: parse arguments, run the selected command, and map the outcome
/// onto a process exit code.
///
/// Centralizing error presentation here (rather than in `main`) means every binary —
/// including the `dog` alias — inherits identical, format-aware error handling.
pub fn run() -> i32 {
    let cli = Cli::parse();

    // Resolve the global presentation switches before anything can render.
    output::init(cli.color);

    match run_command(&cli) {
        Ok(code) => code,
        Err(err) => {
            emit_fatal_error(cli.format, &err);
            2
        }
    }
}

fn run_command(cli: &Cli) -> Result<i32> {
    // Bare engine version, printed before any workspace/spec resolution so that
    // `docgov --version` is a pure, offline, side-effect-free query. The bytes on
    // stdout are exactly the version string (no program-name prefix, no color).
    if cli.version {
        println!("{}", CLI_VERSION);
        return Ok(0);
    }

    let target_dir = cli.path.clone().unwrap_or_else(|| PathBuf::from("."));

    match cli.command.as_ref() {
        None => run_check(
            &target_dir,
            cli.format,
            false,
            None,
            None,
            None,
            /* notifier allowed */ cli.format == OutputFormat::Text,
        ),
        Some(Commands::Check {
            diff,
            base,
            baseline,
            generate_baseline,
        }) => run_check(
            &target_dir,
            cli.format,
            *diff,
            base.as_deref(),
            baseline.clone(),
            generate_baseline.clone(),
            cli.format == OutputFormat::Text,
        ),
        Some(Commands::Diff { base }) => run_diff(&target_dir, cli.format, base),
        Some(Commands::Init { force }) => {
            init_repo(&target_dir, *force)?;
            Ok(0)
        }
        Some(Commands::Update {
            target,
            check,
            dry_run,
            force,
            local,
        }) => update_repo(
            &target_dir,
            target.as_deref(),
            *check,
            *dry_run,
            *force,
            *local,
        ),
    }
}

/// Execute a lint pass and render the report.
///
/// `notify` gates the advisory "new spec available" hint; it is only ever true for the
/// `text` format, keeping machine-readable streams free of decoration.
fn run_check(
    target_dir: &std::path::Path,
    format: OutputFormat,
    diff: bool,
    base: Option<&str>,
    baseline: Option<PathBuf>,
    generate_baseline: Option<PathBuf>,
    notify: bool,
) -> Result<i32> {
    let start = std::time::Instant::now();
    let engine = LintEngine::new(target_dir)?;

    let changed = if diff || base.is_some() {
        Some(get_changed_files(target_dir, base)?)
    } else {
        None
    };

    let raw_diagnostics = engine.run_lint(changed.as_deref())?;

    if let Some(baseline_out) = generate_baseline {
        let base_obj =
            crate::baseline::Baseline::from_diagnostics(&engine.config.version, &raw_diagnostics);
        let out_path = if baseline_out.is_absolute() {
            baseline_out
        } else {
            target_dir.join(baseline_out)
        };
        base_obj.save_to_file(&out_path)?;
        println!(
            "{}",
            output::status(
                Sym::Success,
                format!(
                    "Generated baseline with {} at {}",
                    output::plural(base_obj.entries.len(), "violation", "violations"),
                    out_path.display()
                )
            )
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

    output_diagnostics(
        &diagnostics,
        format,
        duration,
        suppressed_count,
        &engine.config.version,
    );

    let error_count = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();

    if notify {
        check_for_spec_update_hint(target_dir, &engine.config);
    }

    Ok(if error_count > 0 { 1 } else { 0 })
}

/// Execute the standalone git-diff trigger check (`docgov diff`).
fn run_diff(target_dir: &std::path::Path, format: OutputFormat, base: &str) -> Result<i32> {
    let start = std::time::Instant::now();
    let engine = LintEngine::new(target_dir)?;
    let changed = get_changed_files(target_dir, Some(base))?;

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

    output_diagnostics(&diagnostics, format, duration, 0, &engine.config.version);

    let error_count = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    Ok(if error_count > 0 { 1 } else { 0 })
}

/// Render the final report for a lint pass, in the requested output format.
///
/// Only the `text` format emits timing, decoration, or summaries. `github` and `json`
/// are pure data channels so they can be consumed by CI without post-processing, and
/// so that redirecting them to a file yields a byte-reproducible artifact.
fn output_diagnostics(
    diagnostics: &[Diagnostic],
    format: OutputFormat,
    duration: std::time::Duration,
    suppressed_count: usize,
    protocol_version: &str,
) {
    match format {
        OutputFormat::Text => {
            let summary = ReportSummary::tally(diagnostics, suppressed_count);

            if diagnostics.is_empty() {
                let mut line = output::status(
                    Sym::Success,
                    format!(
                        "All Protocol v{} documentation invariants verified in {:.3}s.",
                        protocol_version,
                        duration.as_secs_f64()
                    ),
                );
                if suppressed_count > 0 {
                    line.push_str(&format!(
                        " ({} suppressed by baseline)",
                        output::plural(suppressed_count, "known violation", "known violations")
                    ));
                }
                println!("{}", line);
                return;
            }

            println!();
            for diag in diagnostics {
                println!("{}", diag.render_terminal());
            }

            let counts = format!(
                "{}, {}",
                output::plural(summary.errors, "error", "errors"),
                output::plural(summary.warnings, "warning", "warnings")
            );
            let mut verdict = output::status(
                Sym::Error,
                format!(
                    "Found {} in {:.3}s. (Check failed)",
                    counts,
                    duration.as_secs_f64()
                ),
            );
            if suppressed_count > 0 {
                verdict.push_str(&format!(
                    " ({} suppressed by baseline)",
                    output::plural(suppressed_count, "violation", "violations")
                ));
            }
            println!("{}\n", verdict);
        }
        OutputFormat::Github => {
            for diag in diagnostics {
                println!("{}", diag.render_github());
            }
        }
        OutputFormat::Json => {
            println!(
                "{}",
                render_json_report(diagnostics, suppressed_count, protocol_version)
            );
        }
    }
}

/// Serialize the machine-readable report envelope.
///
/// Extracted from [`output_diagnostics`] so the failure path in `run()` can emit the
/// same envelope (with a zeroed summary) instead of a bare array — a consumer that
/// always parses one shape is far easier to write than one that must sniff for two.
fn render_json_report(
    diagnostics: &[Diagnostic],
    suppressed_count: usize,
    protocol_version: &str,
) -> String {
    let report = DiagnosticReport {
        schema_version: JSON_SCHEMA_VERSION,
        protocol_version: protocol_version.to_string(),
        summary: ReportSummary::tally(diagnostics, suppressed_count),
        diagnostics,
    };
    serde_json::to_string_pretty(&report)
        .unwrap_or_else(|_| "{\"schema_version\":1,\"diagnostics\":[]}".to_string())
}

/// Emit a fatal engine error, honoring the requested output format.
///
/// In `json` mode the error is delivered as a machine-readable envelope on stdout so
/// a CI consumer never has to scrape stderr prose; the human-readable form always
/// goes to stderr.
fn emit_fatal_error(format: OutputFormat, err: &anyhow::Error) {
    if format == OutputFormat::Json {
        let payload = serde_json::json!({
            "schema_version": JSON_SCHEMA_VERSION,
            "error": format!("{:#}", err),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );
    }
    eprintln!("{} {:#}", output::red("Error:"), err);
}

fn init_repo(dir: &std::path::Path, force: bool) -> Result<()> {
    let docgov_yml = dir.join(".docgov.yml");

    if !docgov_yml.exists() || force {
        let (cfg, _) = Config::load_from_dir(dir).unwrap_or((Config::default(), None));
        let remote_client = RemoteClient::new(&cfg.upstream.source, &cfg.upstream.r#ref);
        let yml_content = match remote_client.fetch_config_template(Some(dir)) {
            Ok((content, _)) => content,
            Err(_) => {
                // Minimal standalone fallback only if completely unreachable
                r#"version: "0.0.2"

upstream:
  source: "https://github.com/ming2k/docgov"
  ref: "v0.0.2"

governance_docs:
  install: true
  target_dir: "docs/governance/documentation"

agent_directives:
  enforce: true
  targets:
    - "AGENTS.md"
"#
                .to_string()
            }
        };

        std::fs::write(&docgov_yml, yml_content)?;
        println!(
            "{}",
            output::status(Sym::Create, format!("Created {}", docgov_yml.display()))
        );
    } else {
        println!(
            "{}",
            output::status(Sym::Update, format!("Exists: {}", docgov_yml.display()))
        );
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
                    "{}",
                    output::status(
                        Sym::Create,
                        format!(
                            "Created {} with docgov directives block",
                            target_path.display()
                        )
                    )
                );
            }
            PatchAction::Appended => {
                std::fs::write(&target_path, &new_content)?;
                println!(
                    "{}",
                    output::status(
                        Sym::Create,
                        format!(
                            "Non-invasively inserted docgov directives block into {}",
                            target_path.display()
                        )
                    )
                );
            }
            PatchAction::Updated => {
                std::fs::write(&target_path, &new_content)?;
                println!(
                    "{}",
                    output::status(
                        Sym::Update,
                        format!(
                            "Updated docgov directives block in {}",
                            target_path.display()
                        )
                    )
                );
            }
            PatchAction::Unchanged => {
                if force {
                    std::fs::write(&target_path, &new_content)?;
                    println!(
                        "{}",
                        output::status(
                            Sym::Update,
                            format!(
                                "Refreshed docgov directives block in {}",
                                target_path.display()
                            )
                        )
                    );
                } else {
                    println!(
                        "{}",
                        output::status(
                            Sym::Success,
                            format!("Directives up to date: {}", target_path.display())
                        )
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
        "{}",
        output::status(
            Sym::Success,
            format!("Updated {}", DocgovLock::lockfile_path(dir).display())
        )
    );

    Ok(())
}

pub fn update_repo(
    dir: &std::path::Path,
    target: Option<&str>,
    check: bool,
    dry_run: bool,
    force: bool,
    local: bool,
) -> Result<i32> {
    if local {
        println!(
            "{}",
            output::status(
                Sym::Info,
                "Synchronizing from local specification directory (spec/)..."
            )
        );
        sync_repo(dir, force, true)?;
        return Ok(0);
    }
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
            "{}",
            output::status(
                Sym::Info,
                format!(
                    "Upstream specification source automatically migrated: {} -> {}",
                    cfg.upstream.source, canonical_source
                )
            )
        );
    }

    println!(
        "{}",
        output::status(
            Sym::Info,
            format!(
                "Resolving upstream specification for {} (current: {})...",
                canonical_source, current_ref
            )
        )
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
            output::red("error[COMPAT]:"),
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
                        "{}",
                        output::status(
                            Sym::Success,
                            format!("Upstream specification is up to date: {}", current_ref)
                        )
                    );
                    return Ok(0);
                }
                println!(
                    "{}",
                    output::status(
                        Sym::Success,
                        format!(
                            "Repository specification is already up to date: {} ({})",
                            current_ref, canonical_source
                        )
                    )
                );
                return Ok(0);
            }
        }
        std::cmp::Ordering::Less => {
            if !force && !migrated {
                if check {
                    println!(
                        "{}",
                        output::status(
                            Sym::Success,
                            format!(
                                "Local specification is ahead of target: {} > {}",
                                current_ref, target_tag
                            )
                        )
                    );
                    return Ok(0);
                }
                println!(
                    "{}",
                    output::status(
                        Sym::Update,
                        format!(
                            "Repository specification ({}) is newer than target ({}) (use --force to downgrade)",
                            current_ref, target_tag
                        )
                    )
                );
                return Ok(0);
            }
        }
        std::cmp::Ordering::Greater => {}
    }

    // New version detected or forced
    if check {
        println!(
            "{}",
            output::status(
                Sym::Info,
                format!(
                    "New specification version available: {} -> {} (Run 'docgov update' to upgrade)",
                    current_ref, target_tag
                )
            )
        );
        return Ok(1); // Standard check mode: return non-zero exit code when updates are available
    }

    if dry_run {
        println!();
        println!(
            "{}",
            output::status(Sym::Update, "Specification Upgrade Preview (dry run):")
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
        "{}",
        output::status(
            Sym::Info,
            format!(
                "Upgrading specification: {} -> {}",
                output::yellow(&current_ref),
                output::green(&target_tag)
            )
        )
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
            "{}",
            output::status(
                Sym::Update,
                format!(
                    "Updated specification declaration in {}",
                    docgov_yml_path.display()
                )
            )
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
        "{}",
        output::status(
            Sym::Success,
            format!(
                "Successfully updated specification to {}!",
                output::green(&target_tag)
            )
        )
    );
    println!(
        "{}",
        output::status(
            Sym::Info,
            format!(
                "Run '{}' to verify repository compliance under the new specification.",
                output::bold("docgov check")
            )
        )
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
    // Advisory chatter is only ever printed to an interactive terminal, and only in
    // `text` mode — it must never contaminate a machine-readable stream. Callers that
    // request `github`/`json` therefore skip this hint entirely.
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
                "{}",
                output::status(
                    Sym::Info,
                    format!(
                        "A new governance specification is available: {} -> {} (Run 'docgov update' to upgrade)",
                        output::yellow(current_ref),
                        output::green(&info.tag_name)
                    )
                )
            );
        }
    }
}
