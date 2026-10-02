use docgov::config::Config;
use docgov::engine::LintEngine;
use docgov::rules::lint_04_trigger::GitSyncTriggerRule;
use docgov::rules::lint_05_agent_directives::{
    patch_agent_directives, PatchAction, DOCGOV_DIRECTIVES_BEGIN, DOCGOV_DIRECTIVES_END,
};
use docgov::rules::LintContext;
use docgov::rules::Rule;
use std::fs;
use tempfile::tempdir;

const SAMPLE_SNIPPET: &str = include_str!("fixtures/directives.snippet");

#[test]
fn test_lint_01_root_sanitizer() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Create allowed file and unapproved file
    fs::write(root.join("README.md"), "# Hello").unwrap();
    fs::write(root.join("FORBIDDEN.md"), "# Secret Notes").unwrap();

    let engine = LintEngine::new(root).unwrap();
    let diagnostics = engine.run_lint(None).unwrap();

    let root_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule_id == "INV-LINT-01")
        .collect();

    assert_eq!(root_errors.len(), 1);
    assert!(root_errors[0].message.contains("FORBIDDEN.md"));
}

#[test]
fn test_lint_02_contributor_firewall() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Setup public and internal docs
    let tut_dir = root.join("docs/tutorials");
    let dev_dir = root.join("docs/dev");
    fs::create_dir_all(&tut_dir).unwrap();
    fs::create_dir_all(&dev_dir).unwrap();

    fs::write(dev_dir.join("setup.md"), "# Setup").unwrap();
    // Public doc linking into internal dev space
    fs::write(
        tut_dir.join("quickstart.md"),
        "# Quickstart\n\nFor details, see [Dev Setup](../dev/setup.md).\n",
    )
    .unwrap();

    let engine = LintEngine::new(root).unwrap();
    let diagnostics = engine.run_lint(None).unwrap();

    let firewall_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule_id == "INV-LINT-02")
        .collect();

    assert_eq!(firewall_errors.len(), 1);
    assert_eq!(firewall_errors[0].line, Some(3));
    assert!(firewall_errors[0].message.contains("docs/dev/setup.md"));
}

#[test]
fn test_lint_03_frontmatter_validation() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let adr_dir = root.join("docs/adr");
    fs::create_dir_all(&adr_dir).unwrap();

    // 1. Missing frontmatter
    fs::write(adr_dir.join("0001-no-fm.md"), "# ADR 1\nContent").unwrap();

    // 2. Missing mandatory field
    fs::write(
        adr_dir.join("0002-missing-id.md"),
        "---\ntitle: Foo\nstatus: accepted\ndate: 2026-08-25\n---\n# ADR 2",
    )
    .unwrap();

    // 3. Invalid status enum
    fs::write(
        adr_dir.join("0003-bad-status.md"),
        "---\nid: ADR-0003\ntitle: Bar\nstatus: finished\ndate: 2026-08-25\n---\n# ADR 3",
    )
    .unwrap();

    // 4. Superseded without superseded_by
    fs::write(
        adr_dir.join("0004-superseded-no-pointer.md"),
        "---\nid: ADR-0004\ntitle: Baz\nstatus: superseded\ndate: 2026-08-25\n---\n# ADR 4",
    )
    .unwrap();

    // 5. Valid ADR
    fs::write(
        adr_dir.join("0005-valid.md"),
        "---\nid: ADR-0005\ntitle: Qux\nstatus: accepted\ndate: 2026-08-25\n---\n# ADR 5",
    )
    .unwrap();

    let engine = LintEngine::new(root).unwrap();
    let diagnostics = engine.run_lint(None).unwrap();

    let adr_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule_id == "INV-LINT-03")
        .collect();

    assert_eq!(adr_errors.len(), 4);
    assert!(adr_errors
        .iter()
        .any(|d| d.message.contains("Missing YAML frontmatter")));
    assert!(adr_errors.iter().any(|d| d
        .message
        .contains("Missing mandatory frontmatter field: 'id'")));
    assert!(adr_errors
        .iter()
        .any(|d| d.message.contains("Invalid status 'finished'")));
    assert!(adr_errors.iter().any(|d| d
        .message
        .contains("must provide a valid 'superseded_by' pointer")));
}

#[test]
fn test_lint_04_git_sync_trigger() {
    let config = Config {
        triggers: vec![docgov::config::TriggerConfig {
            watch: "src/api/**".to_string(),
            require_update: vec!["docs/reference/**".to_string()],
            message: None,
        }],
        ..Default::default()
    };

    let root_path = std::path::PathBuf::from("/test");

    // Case 1: src/api modified but docs/reference NOT modified -> Error
    let changed1 = vec![std::path::PathBuf::from("src/api/auth.rs")];
    let ctx1 = LintContext {
        workspace_root: &root_path,
        config: &config,
        root_files: &[],
        markdown_docs: &std::collections::HashMap::new(),
        changed_files: Some(&changed1),
    };
    let diags1 = GitSyncTriggerRule.check(&ctx1).unwrap();
    assert_eq!(diags1.len(), 1);
    assert_eq!(diags1[0].rule_id, "INV-LINT-04");

    // Case 2: src/api modified and docs/reference also modified -> Pass
    let changed2 = vec![
        std::path::PathBuf::from("src/api/auth.rs"),
        std::path::PathBuf::from("docs/reference/auth.md"),
    ];
    let ctx2 = LintContext {
        workspace_root: &root_path,
        config: &config,
        root_files: &[],
        markdown_docs: &std::collections::HashMap::new(),
        changed_files: Some(&changed2),
    };
    let diags2 = GitSyncTriggerRule.check(&ctx2).unwrap();
    assert_eq!(diags2.len(), 0);
}

#[test]
fn test_render_formats() {
    let diag = docgov::Diagnostic::error(
        "INV-LINT-02",
        "Public doc links into internal dev",
        "docs/tutorials/intro.md",
    )
    .with_location(14, 5)
    .with_snippet("[Dev](../dev/setup.md)")
    .with_suggestion("Move concept to public doc");

    let gh = diag.render_github();
    assert_eq!(
        gh,
        "::error file=docs/tutorials/intro.md,line=14,col=5,title=[INV-LINT-02]::Public doc links into internal dev"
    );

    let term = diag.render_terminal();
    assert!(term.contains("[INV-LINT-02]"));
    assert!(term.contains("docs/tutorials/intro.md:14:5"));
}

#[test]
fn test_lint_05_agent_directives_rule() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // 1. Missing target file -> error
    let engine = LintEngine::new(root).unwrap();
    let diagnostics = engine.run_lint(None).unwrap();
    let errs: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule_id == "INV-LINT-05")
        .collect();
    assert_eq!(errs.len(), 1);
    assert!(errs[0]
        .message
        .contains("Missing agent directives configuration file"));

    // 2. Existing file without docgov marker block -> error
    fs::write(
        root.join("AGENTS.md"),
        "# Project Instructions\n\nCustom rules here.\n",
    )
    .unwrap();
    let diagnostics = engine.run_lint(None).unwrap();
    let errs: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule_id == "INV-LINT-05")
        .collect();
    assert_eq!(errs.len(), 1);
    assert!(errs[0]
        .message
        .contains("Missing docgov anchor directives block"));

    // 3. File with marker block -> passes
    let (patched, action) = patch_agent_directives(
        Some(&fs::read_to_string(root.join("AGENTS.md")).unwrap()),
        SAMPLE_SNIPPET,
    );
    assert_eq!(action, PatchAction::Appended);
    fs::write(root.join("AGENTS.md"), patched).unwrap();

    let diagnostics = engine.run_lint(None).unwrap();
    let errs: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule_id == "INV-LINT-05")
        .collect();
    assert_eq!(errs.len(), 0);
}

#[test]
fn test_patch_agent_directives_preserves_single_h1_and_existing_content() {
    let original =
        "# My Custom Coding Assistant Guidelines\n\n## Section 1\nSome developer instructions.\n";
    let (patched, action) = patch_agent_directives(Some(original), SAMPLE_SNIPPET);

    assert_eq!(action, PatchAction::Appended);
    // Preserves original content verbatim
    assert!(patched.starts_with(original.trim_end()));
    // Contains the markers
    assert!(patched.contains(DOCGOV_DIRECTIVES_BEGIN));
    assert!(patched.contains(DOCGOV_DIRECTIVES_END));

    // Crucial check: MUST NOT usurp or introduce another top-level '#' (H1) heading!
    let h1_lines: Vec<&str> = patched
        .lines()
        .filter(|l| l.starts_with("# ") && !l.starts_with("## "))
        .collect();
    assert_eq!(
        h1_lines.len(),
        1,
        "There must be exactly ONE top-level H1 heading in the document!"
    );
    assert_eq!(h1_lines[0], "# My Custom Coding Assistant Guidelines");

    // The inserted section must use secondary heading level (H2: '## Documentation Governance Directives')
    assert!(patched.contains("## Documentation Governance Directives"));

    // Test idempotency: running again on patched text returns Unchanged
    let (reこと, action2) = patch_agent_directives(Some(&patched), SAMPLE_SNIPPET);
    assert_eq!(action2, PatchAction::Unchanged);
    assert_eq!(reこと, patched);

    // Test update: modifying the interior of the marker block is cleanly updated
    let mutated = patched.replace("### 1. Machine Invariants", "### 1. Outdated Invariants");
    let (updated, action3) = patch_agent_directives(Some(&mutated), SAMPLE_SNIPPET);
    assert_eq!(action3, PatchAction::Updated);
    assert!(updated.contains("### 1. Machine Invariants"));
    assert!(!updated.contains("### 1. Outdated Invariants"));
    assert!(updated.starts_with(original.trim_end()));

    // Test creation from None: creates pure snippet without redundant H1 header
    let (created, action4) = patch_agent_directives(None, SAMPLE_SNIPPET);
    assert_eq!(action4, PatchAction::Created);
    assert_eq!(created, format!("{}\n", SAMPLE_SNIPPET.trim()));
    assert!(created.contains("## Documentation Governance Directives"));
}

#[test]
fn test_patch_agent_directives_heals_stripped_delimiters() {
    // When an AI strips boundary comments but leaves or reorganizes the directives section
    let corrupted = "# Agent Directives\n\n## Custom Project Rules\n- Some rule\n\n## Documentation Governance Directives\n\n### 1. Machine Invariants\n- Some corrupted content\n";
    let (healed, action) = patch_agent_directives(Some(corrupted), SAMPLE_SNIPPET);
    assert_eq!(action, PatchAction::Updated);
    assert!(healed.contains(DOCGOV_DIRECTIVES_BEGIN));
    assert!(healed.contains(DOCGOV_DIRECTIVES_END));
    assert!(healed.contains("## Custom Project Rules"));
    // Must NOT contain duplicate "## Documentation Governance Directives"
    assert_eq!(
        healed
            .matches("## Documentation Governance Directives")
            .count(),
        1
    );

    // Also test when begin delimiter exists but closing delimiter was removed
    let missing_end = "# Agent Directives\n\n<!-- BEGIN DOCGOV DIRECTIVES -->\n## Documentation Governance Directives\nCorrupted content without closing tag\n";
    let (healed2, action2) = patch_agent_directives(Some(missing_end), SAMPLE_SNIPPET);
    assert_eq!(action2, PatchAction::Updated);
    assert!(healed2.contains(DOCGOV_DIRECTIVES_BEGIN));
    assert!(healed2.contains(DOCGOV_DIRECTIVES_END));
    assert_eq!(
        healed2
            .matches("## Documentation Governance Directives")
            .count(),
        1
    );
}

#[test]
fn test_lint_05_detects_stripped_boundary_comments() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // AI stripped boundary comments but left directives content
    let content_without_delimiters =
        "# Instructions\n\n## Documentation Governance Directives\nSome directives\n";
    fs::write(root.join("AGENTS.md"), content_without_delimiters).unwrap();

    let engine = LintEngine::new(root).unwrap();
    let diagnostics = engine.run_lint(None).unwrap();
    let errs: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule_id == "INV-LINT-05")
        .collect();
    assert_eq!(errs.len(), 1);
    assert!(errs[0]
        .message
        .contains("boundary comments were stripped or corrupted"));

    // AI kept begin comment but stripped end comment
    let content_missing_end = format!(
        "# Instructions\n\n{}\n## Directives\n",
        DOCGOV_DIRECTIVES_BEGIN
    );
    fs::write(root.join("AGENTS.md"), content_missing_end).unwrap();

    let diagnostics2 = engine.run_lint(None).unwrap();
    let errs2: Vec<_> = diagnostics2
        .iter()
        .filter(|d| d.rule_id == "INV-LINT-05")
        .collect();
    assert_eq!(errs2.len(), 1);
    assert!(errs2[0].message.contains("missing closing delimiter tag"));
}

#[test]
fn test_lockfile_drift_detection() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let (patched, _) = patch_agent_directives(None, SAMPLE_SNIPPET);
    fs::write(root.join("AGENTS.md"), &patched).unwrap();

    let mut lock =
        docgov::lockfile::DocgovLock::new("0.0.1", "https://github.com/ming2k/docgov", "v0.0.1");
    // Purposely set an old/invalid hash in lockfile
    lock.artifacts.agent_directives = Some(docgov::lockfile::ArtifactEntry {
        target: "AGENTS.md".to_string(),
        hash: "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string(),
    });
    lock.save_to_dir(root).unwrap();

    let engine = LintEngine::new(root).unwrap();
    let diagnostics = engine.run_lint(None).unwrap();
    let warnings: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule_id == "INV-LINT-05" && d.severity == docgov::Severity::Warning)
        .collect();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("drifted from .docgov.lock"));
}

#[test]
fn test_sync_governance_docs_atomic_prune() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // 0. Setup local specification fixture in workspace
    let spec_dir = root.join("spec");
    fs::create_dir_all(spec_dir.join("core")).unwrap();
    fs::write(spec_dir.join("directives.snippet"), SAMPLE_SNIPPET).unwrap();
    fs::write(
        spec_dir.join("core/invariants.md"),
        "# Canonical Invariants\n",
    )
    .unwrap();
    fs::write(spec_dir.join("core/taxonomy.md"), "# Canonical Taxonomy\n").unwrap();

    // 1. Create a business doc outside the managed mirror
    let user_adr = root.join("docs/adr/0001-my-feature.md");
    fs::create_dir_all(user_adr.parent().unwrap()).unwrap();
    fs::write(&user_adr, "# Business ADR\n").unwrap();

    // 2. Create an obsolete ghost file inside the managed mirror
    let ghost_file = root.join("docs/governance/documentation/obsolete_ghost_rule.md");
    fs::create_dir_all(ghost_file.parent().unwrap()).unwrap();
    fs::write(
        &ghost_file,
        "This is an obsolete rule from an older version\n",
    )
    .unwrap();

    assert!(ghost_file.exists());

    // 3. Run sync_governance_docs from local repository spec
    let client = docgov::remote::RemoteClient::new(".", "local");
    client
        .sync_governance_docs(root, "docs/governance/documentation", true)
        .unwrap();

    // 4. Verify canonical files were unpacked
    assert!(root
        .join("docs/governance/documentation/core/invariants.md")
        .exists());
    assert!(root
        .join("docs/governance/documentation/core/taxonomy.md")
        .exists());

    // 5. Verify the obsolete ghost file was completely pruned!
    assert!(
        !ghost_file.exists(),
        "Obsolete file must be pruned during full atomic replacement!"
    );

    // 6. Verify user's business doc was completely untouched!
    assert!(user_adr.exists());
    assert_eq!(fs::read_to_string(&user_adr).unwrap(), "# Business ADR\n");
}

#[test]
fn test_remote_client_fails_deterministically_without_embedded_fallback() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Point to a non-existent remote repo and non-existent version
    let client = docgov::remote::RemoteClient::new(
        "https://github.com/nonexistent-org-99999/nonexistent-repo-99999",
        "v99.99.99",
    );

    // Fetching directives must fail explicitly, never silently falling back to embedded constants!
    let directives_res = client.fetch_directives(None);
    assert!(
        directives_res.is_err(),
        "Must fail when remote endpoint is not found, never silently fallback!"
    );
    let err_msg = directives_res.err().unwrap().to_string();
    assert!(
        err_msg.contains("Failed to fetch directives snippet for upstream"),
        "Error message must be clear: {err_msg}"
    );

    // Syncing governance docs must fail explicitly without embedded tar fallback!
    let sync_res = client.sync_governance_docs(root, "docs/governance/documentation", true);
    assert!(
        sync_res.is_err(),
        "Must fail when remote release archive is not found, never silently fallback!"
    );
    let err_msg = sync_res.err().unwrap().to_string();
    assert!(
        err_msg.contains("Failed to download governance documentation assets"),
        "Error message must be clear: {err_msg}"
    );
}

#[test]
fn test_local_spec_sync_deterministic_hash_and_directives() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Setup local spec in workspace
    let spec_dir = root.join("spec");
    fs::create_dir_all(spec_dir.join("core")).unwrap();
    let snippet_content = "<!-- BEGIN DOCGOV DIRECTIVES -->\n## Custom Local Directives\n<!-- END DOCGOV DIRECTIVES -->";
    fs::write(spec_dir.join("directives.snippet"), snippet_content).unwrap();
    fs::write(spec_dir.join("core/invariants.md"), "# Local Invariants\n").unwrap();

    let client = docgov::remote::RemoteClient::new(".", "local");
    let (fetched_snippet, source_info) = client.fetch_directives(Some(root)).unwrap();
    assert_eq!(fetched_snippet, snippet_content);
    assert!(source_info.starts_with("local:"));

    let hash = client
        .sync_governance_docs(root, "docs/governance/documentation", true)
        .unwrap();
    assert!(hash.starts_with("sha256:"));

    // Verify documentation was created from local spec
    assert!(root
        .join("docs/governance/documentation/core/invariants.md")
        .exists());
    assert_eq!(
        fs::read_to_string(root.join("docs/governance/documentation/core/invariants.md")).unwrap(),
        "# Local Invariants\n"
    );
    // Directives snippet must not leak into documentation mirror
    assert!(!root
        .join("docs/governance/documentation/directives.snippet")
        .exists());
}

#[test]
fn test_xdg_cache_dir_resolution() {
    let client = docgov::remote::RemoteClient::new("https://github.com/ming2k/docgov", "v0.0.7");

    // Case 1: When XDG_CACHE_HOME is explicitly set, it must be strictly prioritized
    let custom_cache = tempdir().unwrap();
    std::env::set_var("XDG_CACHE_HOME", custom_cache.path());
    let cache_dir = client.get_cache_dir();
    assert!(
        cache_dir.starts_with(custom_cache.path()),
        "Cache dir must reside within XDG_CACHE_HOME when set! Got: {}",
        cache_dir.display()
    );
    assert!(cache_dir.to_string_lossy().contains("docgov"));
    assert!(cache_dir.to_string_lossy().contains("v0.0.7"));

    // Case 2: Clean up env var
    std::env::remove_var("XDG_CACHE_HOME");
    let fallback_dir = client.get_cache_dir();
    assert!(
        fallback_dir.to_string_lossy().contains("docgov"),
        "Fallback cache dir must still be namespaced under docgov"
    );
}

#[test]
fn test_lint_03_tolerant_status_parsing() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let adr_dir = root.join("docs/adr");
    fs::create_dir_all(&adr_dir).unwrap();

    // 1. Status with amendment note: Accepted — amended by ADR-0020
    fs::write(
        adr_dir.join("0001-accepted-amended.md"),
        "---\nid: ADR-0001\ntitle: Slab Allocator\nstatus: \"Accepted — amended by ADR-0020\"\ndate: 2026-05-21\n---\n# ADR 1",
    )
    .unwrap();

    // 2. Status with superseded pointer in status: Superseded by ADR-0011
    fs::write(
        adr_dir.join("0002-superseded-in-status.md"),
        "---\nid: ADR-0002\ntitle: Tessellator\nstatus: \"Superseded by ADR-0011\"\ndate: 2026-05-20\n---\n# ADR 2",
    )
    .unwrap();

    let engine = LintEngine::new(root).unwrap();
    let diagnostics = engine.run_lint(None).unwrap();

    let adr_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule_id == "INV-LINT-03")
        .collect();

    assert_eq!(
        adr_errors.len(),
        0,
        "Tolerant status parsing must accept annotated statuses without error! Found: {:?}",
        adr_errors
    );
}

#[test]
fn test_frontmatter_enforce_and_exclude_switches() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let adr_dir = root.join("docs/adr");
    let legacy_dir = adr_dir.join("legacy");
    fs::create_dir_all(&legacy_dir).unwrap();

    // Invalid ADR without frontmatter in legacy dir
    fs::write(legacy_dir.join("0001-legacy.md"), "# Legacy ADR").unwrap();

    // 1. Exclude pattern suppresses check for that subtree
    let mut config = Config::default();
    config.architecture.require_frontmatter.exclude = vec!["docs/adr/legacy/**".to_string()];

    let engine = LintEngine::new(root).unwrap().with_config(config);
    let diagnostics = engine.run_lint(None).unwrap();
    let adr_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule_id == "INV-LINT-03")
        .collect();
    assert_eq!(
        adr_errors.len(),
        0,
        "Excluded path must not report frontmatter errors"
    );

    // 2. Enforce = false disables frontmatter rule entirely
    let mut config_disabled = Config::default();
    config_disabled.architecture.require_frontmatter.enforce = false;

    let engine_disabled = LintEngine::new(root).unwrap().with_config(config_disabled);
    let diagnostics_disabled = engine_disabled.run_lint(None).unwrap();
    let adr_errors_disabled: Vec<_> = diagnostics_disabled
        .iter()
        .filter(|d| d.rule_id == "INV-LINT-03")
        .collect();
    assert_eq!(
        adr_errors_disabled.len(),
        0,
        "Disabled enforce must produce zero frontmatter diagnostics"
    );
}

#[test]
fn test_baseline_generation_and_filtering() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let adr_dir = root.join("docs/adr");
    fs::create_dir_all(&adr_dir).unwrap();

    // Create 2 ADRs missing frontmatter
    fs::write(adr_dir.join("0001-old.md"), "# Old ADR 1").unwrap();
    fs::write(adr_dir.join("0002-old.md"), "# Old ADR 2").unwrap();

    let mut config = Config::default();
    config.agent_directives.enforce = false;

    let engine = LintEngine::new(root).unwrap().with_config(config.clone());
    let diagnostics = engine.run_lint(None).unwrap();
    assert_eq!(diagnostics.len(), 2);

    // Generate baseline
    let baseline = docgov::baseline::Baseline::from_diagnostics("0.1.0", &diagnostics);
    let baseline_file = root.join(".docgov-baseline.json");
    baseline.save_to_file(&baseline_file).unwrap();

    // Load and filter
    let loaded = docgov::baseline::Baseline::load_from_file(&baseline_file).unwrap();
    let (remaining, suppressed) = loaded.filter_diagnostics(diagnostics);
    assert_eq!(suppressed, 2);
    assert_eq!(remaining.len(), 0);

    // Add a NEW violation (new unapproved file at root)
    fs::write(root.join("FORBIDDEN.md"), "# New Error").unwrap();
    let new_diagnostics = engine.run_lint(None).unwrap();
    let (remaining_after, suppressed_after) = loaded.filter_diagnostics(new_diagnostics);
    assert_eq!(suppressed_after, 2);
    assert_eq!(remaining_after.len(), 1);
    assert_eq!(remaining_after[0].rule_id, "INV-LINT-01");
}

#[test]
fn test_docgov_fix_frontmatter_synthesis() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let adr_dir = root.join("docs/adr");
    fs::create_dir_all(&adr_dir).unwrap();

    // 1. Classic Michael Nygard / MADR ADR
    fs::write(
        adr_dir.join("0001-project-foundations.md"),
        "# ADR-0001: Project foundations\n\n- Status: Accepted\n- Date: 2026-05-19\n\n## Context\nSome context.",
    )
    .unwrap();

    // 2. Superseded ADR with pointer
    fs::write(
        adr_dir.join("0005-tessellator-scope.md"),
        "# ADR-0005: Tessellator scope\n\n- Status: Superseded by ADR-0011\n- Date: 2026-05-20\n\n## Context\nTessellator details.",
    )
    .unwrap();

    // Run fix
    let summary = docgov::fix::run_fix(root, Some(&["INV-LINT-03".to_string()]), false).unwrap();
    assert_eq!(summary.fixed.len(), 2);

    // Now run lint - it should pass with 0 errors!
    let engine = LintEngine::new(root).unwrap();
    let diagnostics = engine.run_lint(None).unwrap();
    let adr_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule_id == "INV-LINT-03")
        .collect();
    assert_eq!(
        adr_errors.len(),
        0,
        "Fixed files must pass frontmatter linting cleanly! Found: {:?}",
        adr_errors
    );

    // Inspect content of synthesized frontmatter
    let content_0005 = fs::read_to_string(adr_dir.join("0005-tessellator-scope.md")).unwrap();
    assert!(content_0005.contains("id: ADR-0005"));
    assert!(content_0005.contains("status: superseded"));
    assert!(content_0005.contains("superseded_by: \"ADR-0011\""));
    assert!(content_0005.contains("# ADR-0005: Tessellator scope"));
}

// =========================================================================
// `docgov update` — Spec Evolution & Upgrade Contract Tests
// =========================================================================

#[test]
fn test_patch_yaml_version_and_ref_non_invasive() {
    let original = r#"version: "0.0.7"

# Remote Upstream & Protocol Distribution
upstream:
  source: "https://github.com/ming2k/docgov-spec"
  ref: "v0.0.7"

# Canonical Governance Documentation Mirror (for Agent Context)
governance_docs:
  install: true
  target_dir: "docs/governance/documentation"

triggers:
  - watch: "src/api/**"
    require_update:
      - "docs/reference/**"
"#;

    let patched = docgov::cli::patch_yaml_version_and_ref(original, "0.1.0", "v0.1.0");

    // Version and ref must be advanced
    assert!(patched.contains("version: \"0.1.0\""));
    assert!(patched.contains("ref: \"v0.1.0\""));

    // Everything else must remain byte-for-byte untouched (non-invasive)
    assert!(patched.contains("source: \"https://github.com/ming2k/docgov-spec\""));
    assert!(patched.contains("governance_docs:"));
    assert!(patched.contains("target_dir: \"docs/governance/documentation\""));
    assert!(patched.contains("watch: \"src/api/**\""));
    assert!(patched.contains("\"docs/reference/**\""));
    assert!(!patched.contains("v0.0.7"));
    assert!(patched.ends_with('\n'));
}

#[test]
fn test_patch_yaml_version_and_ref_preserves_comments_and_trailing_newline() {
    let original = "# my custom header comment\nversion: \"0.0.7\"\nupstream:\n  source: \".\"\n  ref: \"local\"\n";
    let patched = docgov::cli::patch_yaml_version_and_ref(original, "0.2.0", "v0.2.0");

    assert!(patched.starts_with("# my custom header comment\n"));
    assert!(patched.contains("version: \"0.2.0\""));
    assert!(patched.contains("ref: \"v0.2.0\""));
    assert!(patched.contains("source: \".\""));
    assert!(patched.ends_with('\n'));
}

#[test]
fn test_canonicalize_upstream_source() {
    use docgov::remote::canonicalize_upstream_source;

    let (c1, m1) = canonicalize_upstream_source("https://github.com/ming2k/docs-governance");
    assert_eq!(c1, "https://github.com/ming2k/docgov-spec");
    assert!(m1);

    let (c2, m2) = canonicalize_upstream_source("https://github.com/ming2k/docgov");
    assert_eq!(c2, "https://github.com/ming2k/docgov-spec");
    assert!(m2);

    let (c3, m3) = canonicalize_upstream_source("https://github.com/ming2k/docgov-spec");
    assert_eq!(c3, "https://github.com/ming2k/docgov-spec");
    assert!(!m3);

    let (c4, m4) = canonicalize_upstream_source("https://github.com/my-org/my-spec");
    assert_eq!(c4, "https://github.com/my-org/my-spec");
    assert!(!m4);
}

#[test]
fn test_parse_and_compare_semver_with_tags() {
    use docgov::remote::{compare_semver, parse_semver_parts};

    assert_eq!(parse_semver_parts("spec-v0.1.0"), (0, 1, 0, ""));
    assert_eq!(parse_semver_parts("cli-v0.0.9"), (0, 0, 9, ""));
    assert_eq!(parse_semver_parts("v0.1.0"), (0, 1, 0, ""));
    assert_eq!(parse_semver_parts("0.1.0"), (0, 1, 0, ""));

    assert_eq!(
        compare_semver("spec-v0.1.0", "v0.0.9"),
        std::cmp::Ordering::Greater
    );
    assert_eq!(
        compare_semver("spec-v0.1.0", "spec-v0.1.0"),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        compare_semver("cli-v0.0.9", "cli-v0.0.8"),
        std::cmp::Ordering::Greater
    );
}

#[test]
fn test_patch_yaml_version_ref_and_source_migrates_legacy_upstream() {
    let original = r#"version: "0.0.3"

upstream:
  source: "https://github.com/ming2k/docs-governance"
  ref: "v0.0.3"
"#;
    let patched = docgov::cli::patch_yaml_version_ref_and_source(
        original,
        "0.1.0",
        "v0.1.0",
        Some("https://github.com/ming2k/docgov-spec"),
    );

    assert!(patched.contains("version: \"0.1.0\""));
    assert!(patched.contains("ref: \"v0.1.0\""));
    assert!(patched.contains("source: \"https://github.com/ming2k/docgov-spec\""));
    assert!(!patched.contains("docs-governance"));
    assert!(!patched.contains("v0.0.3"));
}

#[test]
fn test_update_atomic_rollback_on_sync_failure() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let ws = root.join("workspace");
    fs::create_dir_all(&ws).unwrap();

    let initial_yml = "version: \"0.0.3\"\n\nupstream:\n  source: \"https://github.com/nonexistent-org-xyz/nonexistent-repo\"\n  ref: \"v0.0.3\"\n";
    fs::write(ws.join(".docgov.yml"), initial_yml).unwrap();

    // Attempting update to a target that fails to sync
    let res = docgov::cli::update_repo(&ws, Some("v0.9.9"), false, false, true);
    assert!(res.is_err());

    // .docgov.yml must be safely rolled back to initial state
    let rolled_back = fs::read_to_string(ws.join(".docgov.yml")).unwrap();
    assert_eq!(rolled_back, initial_yml);
}

#[test]
fn test_update_check_mode_detects_newer_spec_via_local_upstream() {
    // Scenario: repository locked at an older spec, local sibling spec is newer.
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Fake spec repo with VERSION 0.3.0
    let spec_dir = root.join("docgov-spec");
    fs::create_dir_all(spec_dir.join("core")).unwrap();
    fs::write(spec_dir.join("VERSION"), "0.3.0\n").unwrap();
    fs::write(spec_dir.join("directives.snippet"), SAMPLE_SNIPPET).unwrap();
    fs::write(spec_dir.join("core/invariants.md"), "# Inv\n").unwrap();

    let ws = root.join("workspace");
    fs::create_dir_all(&ws).unwrap();

    // Repository locked at v0.1.0 while local spec resolves to v0.3.0
    let yml = format!(
        "version: \"0.1.0\"\n\nupstream:\n  source: \"{}\"\n  ref: \"v0.1.0\"\n",
        spec_dir.display()
    );
    fs::write(ws.join(".docgov.yml"), yml).unwrap();

    // `--check` must exit non-zero to signal "update available"
    let code = docgov::cli::update_repo(&ws, None, true, false, false).unwrap();
    assert_eq!(code, 1, "--check must exit 1 when an upgrade is available");

    // No files may be mutated in check mode
    let after = fs::read_to_string(ws.join(".docgov.yml")).unwrap();
    assert!(after.contains("v0.1.0"));
    assert!(!ws.join(".docgov.lock").exists());
}

#[test]
fn test_update_check_mode_reports_up_to_date_with_zero_exit() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let spec_dir = root.join("docgov-spec");
    fs::create_dir_all(spec_dir.join("core")).unwrap();
    fs::write(spec_dir.join("VERSION"), "0.1.0\n").unwrap();
    fs::write(spec_dir.join("directives.snippet"), SAMPLE_SNIPPET).unwrap();

    let ws = root.join("workspace");
    fs::create_dir_all(&ws).unwrap();

    let yml = format!(
        "version: \"0.1.0\"\n\nupstream:\n  source: \"{}\"\n  ref: \"v0.1.0\"\n",
        spec_dir.display()
    );
    fs::write(ws.join(".docgov.yml"), yml).unwrap();

    let code = docgov::cli::update_repo(&ws, None, true, false, false).unwrap();
    assert_eq!(code, 0, "--check must exit 0 when already up to date");
}

#[test]
fn test_update_dry_run_makes_no_changes() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let spec_dir = root.join("docgov-spec");
    fs::create_dir_all(spec_dir.join("core")).unwrap();
    fs::write(spec_dir.join("VERSION"), "0.4.0\n").unwrap();
    fs::write(spec_dir.join("directives.snippet"), SAMPLE_SNIPPET).unwrap();
    fs::write(spec_dir.join("core/invariants.md"), "# Inv\n").unwrap();

    let ws = root.join("workspace");
    fs::create_dir_all(&ws).unwrap();

    let yml = format!(
        "version: \"0.1.0\"\n\nupstream:\n  source: \"{}\"\n  ref: \"v0.1.0\"\n",
        spec_dir.display()
    );
    fs::write(ws.join(".docgov.yml"), yml).unwrap();

    let code = docgov::cli::update_repo(&ws, None, false, true, false).unwrap();
    assert_eq!(code, 0);

    // Dry run must not touch the config or produce a lockfile
    let after = fs::read_to_string(ws.join(".docgov.yml")).unwrap();
    assert!(after.contains("v0.1.0"));
    assert!(!ws.join(".docgov.lock").exists());
    assert!(!ws.join("docs/governance/documentation").exists());
}

#[test]
fn test_update_end_to_end_applies_upgrade_and_refreshes_lock() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let spec_dir = root.join("docgov-spec");
    fs::create_dir_all(spec_dir.join("core")).unwrap();
    fs::write(spec_dir.join("VERSION"), "0.5.0\n").unwrap();
    fs::write(spec_dir.join("directives.snippet"), SAMPLE_SNIPPET).unwrap();
    fs::write(spec_dir.join("core/invariants.md"), "# New Invariants\n").unwrap();

    let ws = root.join("workspace");
    fs::create_dir_all(&ws).unwrap();

    let yml = format!(
        "version: \"0.1.0\"\n\nupstream:\n  source: \"{}\"\n  ref: \"v0.1.0\"\n\ngovernance_docs:\n  install: true\n  target_dir: \"docs/governance/documentation\"\n",
        spec_dir.display()
    );
    fs::write(ws.join(".docgov.yml"), yml).unwrap();

    let code = docgov::cli::update_repo(&ws, None, false, false, false).unwrap();
    assert_eq!(code, 0);

    // 1. Declaration advanced non-invasively
    let after = fs::read_to_string(ws.join(".docgov.yml")).unwrap();
    assert!(after.contains("version: \"0.5.0\""), "config: {}", after);
    assert!(after.contains("ref: \"v0.5.0\""), "config: {}", after);
    assert!(after.contains(&spec_dir.display().to_string()));

    // 2. Lockfile renewed at the new protocol version
    let lock = docgov::lockfile::DocgovLock::load_from_dir(&ws)
        .unwrap()
        .expect("lockfile must exist after update");
    assert_eq!(lock.protocol_version, "0.5.0");
    assert_eq!(lock.upstream.r#ref, "v0.5.0");
    assert!(lock.artifacts.agent_directives.is_some());
    assert!(lock.artifacts.governance_docs.is_some());

    // 3. Directives block injected and mirror materialized
    let agents = fs::read_to_string(ws.join("AGENTS.md")).unwrap();
    assert!(agents.contains("BEGIN DOCGOV DIRECTIVES"));
    assert!(ws
        .join("docs/governance/documentation/core/invariants.md")
        .exists());
}

#[test]
fn test_update_explicit_target_version_argument() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let spec_dir = root.join("docgov-spec");
    fs::create_dir_all(spec_dir.join("core")).unwrap();
    fs::write(spec_dir.join("VERSION"), "0.9.0\n").unwrap();
    fs::write(spec_dir.join("directives.snippet"), SAMPLE_SNIPPET).unwrap();

    let ws = root.join("workspace");
    fs::create_dir_all(&ws).unwrap();

    let yml = format!(
        "version: \"0.1.0\"\n\nupstream:\n  source: \"{}\"\n  ref: \"v0.1.0\"\n",
        spec_dir.display()
    );
    fs::write(ws.join(".docgov.yml"), yml).unwrap();

    // Explicit target pinning (accepts both 'v0.6.0' and '0.6.0')
    let code = docgov::cli::update_repo(&ws, Some("0.6.0"), true, false, false).unwrap();
    assert_eq!(code, 1);

    let code = docgov::cli::update_repo(&ws, Some("v0.6.0"), false, true, false).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn test_update_refuses_unsupported_spec_major() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let spec_dir = root.join("docgov-spec");
    fs::create_dir_all(spec_dir.join("core")).unwrap();
    fs::write(spec_dir.join("VERSION"), "1.0.0\n").unwrap();
    fs::write(spec_dir.join("directives.snippet"), SAMPLE_SNIPPET).unwrap();

    let ws = root.join("workspace");
    fs::create_dir_all(&ws).unwrap();

    let yml = format!(
        "version: \"0.1.0\"\n\nupstream:\n  source: \"{}\"\n  ref: \"v0.1.0\"\n",
        spec_dir.display()
    );
    fs::write(ws.join(".docgov.yml"), yml).unwrap();

    // Spec major 1 exceeds SUPPORTED_SPEC_MAJOR (0): must hard-fail without touching files
    let code = docgov::cli::update_repo(&ws, Some("v1.0.0"), false, false, false).unwrap();
    assert_eq!(
        code, 1,
        "Engine must refuse a spec major it cannot faithfully evaluate"
    );

    let after = fs::read_to_string(ws.join(".docgov.yml")).unwrap();
    assert!(after.contains("v0.1.0"), "config must be untouched");
    assert!(!ws.join(".docgov.lock").exists());
}

#[test]
fn test_update_does_not_downgrade_without_force() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let spec_dir = root.join("docgov-spec");
    fs::create_dir_all(spec_dir.join("core")).unwrap();
    fs::write(spec_dir.join("VERSION"), "0.5.0\n").unwrap();
    fs::write(spec_dir.join("directives.snippet"), SAMPLE_SNIPPET).unwrap();

    let ws = root.join("workspace");
    fs::create_dir_all(&ws).unwrap();

    let yml = format!(
        "version: \"0.5.0\"\n\nupstream:\n  source: \"{}\"\n  ref: \"v0.5.0\"\n",
        spec_dir.display()
    );
    fs::write(ws.join(".docgov.yml"), yml).unwrap();

    // Attempt to move backwards: must be a safe no-op
    let code = docgov::cli::update_repo(&ws, Some("v0.2.0"), false, false, false).unwrap();
    assert_eq!(code, 0);

    let after = fs::read_to_string(ws.join(".docgov.yml")).unwrap();
    assert!(
        after.contains("v0.5.0"),
        "must not silently downgrade: {}",
        after
    );
}

#[test]
fn test_semver_ordering_is_strictly_correct() {
    use docgov::remote::compare_semver;
    use std::cmp::Ordering;

    assert_eq!(compare_semver("0.1.0", "0.1.0"), Ordering::Equal);
    assert_eq!(compare_semver("v0.1.0", "0.1.0"), Ordering::Equal);
    assert_eq!(compare_semver("0.1.0", "0.2.0"), Ordering::Less);
    assert_eq!(compare_semver("0.9.0", "0.10.0"), Ordering::Less);
    assert_eq!(compare_semver("1.0.0", "0.99.99"), Ordering::Greater);
    assert_eq!(compare_semver("1.0.0", "1.0.0-rc1"), Ordering::Greater);
    assert_eq!(compare_semver("1.0.0-rc1", "1.0.0"), Ordering::Less);
}

#[test]
fn test_lint_04_require_update_any_of_semantics() {
    // A trigger with multiple `require_update` globs expresses ANY-OF semantics:
    // touching any single listed surface must satisfy the trigger.
    let config = Config {
        triggers: vec![docgov::config::TriggerConfig {
            watch: "src/**".to_string(),
            require_update: vec!["docs/adr/**".to_string(), "README.md".to_string()],
            message: None,
        }],
        ..Default::default()
    };

    let root_path = std::path::PathBuf::from("/test");
    let docs_map = std::collections::HashMap::new();

    // Case 1: src touched, nothing documented -> violation
    let changed = vec![std::path::PathBuf::from("src/main.rs")];
    let ctx = LintContext {
        workspace_root: &root_path,
        config: &config,
        root_files: &[],
        markdown_docs: &docs_map,
        changed_files: Some(&changed),
    };
    assert_eq!(GitSyncTriggerRule.check(&ctx).unwrap().len(), 1);

    // Case 2: src + docs/adr touched (first alternative) -> satisfied
    let changed = vec![
        std::path::PathBuf::from("src/main.rs"),
        std::path::PathBuf::from("docs/adr/0001-x.md"),
    ];
    let ctx = LintContext {
        workspace_root: &root_path,
        config: &config,
        root_files: &[],
        markdown_docs: &docs_map,
        changed_files: Some(&changed),
    };
    assert_eq!(GitSyncTriggerRule.check(&ctx).unwrap().len(), 0);

    // Case 3: src + README.md touched (second alternative) -> satisfied
    let changed = vec![
        std::path::PathBuf::from("src/main.rs"),
        std::path::PathBuf::from("README.md"),
    ];
    let ctx = LintContext {
        workspace_root: &root_path,
        config: &config,
        root_files: &[],
        markdown_docs: &docs_map,
        changed_files: Some(&changed),
    };
    assert_eq!(GitSyncTriggerRule.check(&ctx).unwrap().len(), 0);
}

#[test]
fn test_trigger_config_accepts_scalar_and_list_forms() {
    // Scalar form must deserialize into a single-element vector (backward compatible).
    let scalar = r#"
version: "0.1.0"
triggers:
  - watch: "src/**"
    require_update: "docs/**"
"#;
    let cfg: Config = serde_yaml::from_str(scalar).unwrap();
    assert_eq!(cfg.triggers.len(), 1);
    assert_eq!(cfg.triggers[0].require_update, vec!["docs/**".to_string()]);

    // List form must deserialize into a multi-element vector.
    let list = r#"
version: "0.1.0"
triggers:
  - watch: "src/**"
    require_update:
      - "docs/**"
      - "README.md"
"#;
    let cfg: Config = serde_yaml::from_str(list).unwrap();
    assert_eq!(cfg.triggers.len(), 1);
    assert_eq!(
        cfg.triggers[0].require_update,
        vec!["docs/**".to_string(), "README.md".to_string()]
    );
}
