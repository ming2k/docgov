use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::ast::MarkdownDocument;
use crate::config::Config;
use crate::lockfile::{compute_sha256, ArtifactEntry, DocgovLock};
use crate::remote::RemoteClient;
use crate::rules::lint_03_frontmatter::parse_status_and_pointer;
use crate::rules::lint_05_agent_directives::{
    extract_directives_block, patch_agent_directives, PatchAction,
};

#[derive(Debug, Clone)]
pub struct FixResult {
    pub file_path: PathBuf,
    pub description: String,
}

pub struct FixSummary {
    pub fixed: Vec<FixResult>,
}

pub fn run_fix(
    workspace_root: &Path,
    rules: Option<&[String]>,
    dry_run: bool,
) -> Result<FixSummary> {
    let (config, _) = Config::load_from_dir(workspace_root)?;
    let mut fixed = Vec::new();

    let check_rule = |rule_id: &str| -> bool {
        match rules {
            Some(r) => r.iter().any(|x| x.eq_ignore_ascii_case(rule_id)),
            None => true,
        }
    };

    // 1. Fix INV-LINT-05 (Agent Directives)
    if check_rule("INV-LINT-05") && config.agent_directives.enforce {
        let targets = if config.agent_directives.targets.is_empty() {
            vec!["AGENTS.md".to_string()]
        } else {
            config.agent_directives.targets.clone()
        };

        // Try to fetch directives
        let remote_client = RemoteClient::new(&config.upstream.source, &config.upstream.r#ref);
        let directives_res = remote_client.fetch_directives(Some(workspace_root));

        let directives_content = match directives_res {
            Ok((content, _)) => Some(content),
            Err(_) => {
                if let Some(spec_dir) = remote_client.local_spec_dir(workspace_root) {
                    let snippet_path = spec_dir.join("directives.snippet");
                    std::fs::read_to_string(snippet_path).ok()
                } else {
                    None
                }
            }
        };

        if let Some(directives_content) = directives_content {
            let mut lock = DocgovLock::load_from_dir(workspace_root)?.unwrap_or_else(|| {
                DocgovLock::new(
                    &config.version,
                    &config.upstream.source,
                    &config.upstream.r#ref,
                )
            });

            for target in targets {
                let target_path = workspace_root.join(&target);
                let existing = if target_path.exists() {
                    Some(std::fs::read_to_string(&target_path)?)
                } else {
                    None
                };

                let (new_content, action) =
                    patch_agent_directives(existing.as_deref(), &directives_content);

                if action != PatchAction::Unchanged {
                    if !dry_run {
                        std::fs::write(&target_path, &new_content)?;
                        if let Some(block) = extract_directives_block(&new_content) {
                            let block_hash = compute_sha256(block.trim());
                            lock.artifacts.agent_directives = Some(ArtifactEntry {
                                target: target.clone(),
                                hash: block_hash,
                            });
                            let _ = lock.save_to_dir(workspace_root);
                        }
                    }

                    fixed.push(FixResult {
                        file_path: PathBuf::from(&target),
                        description: match action {
                            PatchAction::Created => {
                                "Created missing agent directives file with canonical block"
                                    .to_string()
                            }
                            PatchAction::Appended => {
                                "Appended canonical agent directives block".to_string()
                            }
                            PatchAction::Updated => {
                                "Updated canonical agent directives block".to_string()
                            }
                            PatchAction::Unchanged => unreachable!(),
                        },
                    });
                }
            }
        }
    }

    // 2. Fix INV-LINT-03 (ADR Frontmatter)
    if check_rule("INV-LINT-03")
        && config.architecture.enforce
        && config.architecture.require_frontmatter.enforce
    {
        let adr_dir = workspace_root.join(&config.architecture.adr_path);
        if adr_dir.exists() {
            let mut entries: Vec<_> = walkdir::WalkDir::new(&adr_dir)
                .follow_links(false)
                .into_iter()
                .flatten()
                .filter(|e| e.file_type().is_file())
                .collect();
            entries.sort_by_key(|e| e.path().to_path_buf());

            for entry in entries {
                let path = entry.path();
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                if file_name == "index.md" || file_name == "README.md" || file_name == "template.md"
                {
                    continue;
                }
                if !path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
                {
                    continue;
                }

                let rel_path = path
                    .strip_prefix(workspace_root)
                    .unwrap_or(path)
                    .to_path_buf();
                let content = std::fs::read_to_string(path)?;
                let doc = MarkdownDocument::parse(&content);

                if doc.frontmatter.is_none() {
                    if let Some(new_content) = synthesize_adr_frontmatter(
                        &content,
                        &file_name,
                        &config.architecture.require_frontmatter.status_enum,
                    ) {
                        if !dry_run {
                            std::fs::write(path, new_content)?;
                        }
                        fixed.push(FixResult {
                            file_path: rel_path,
                            description: "Synthesized standard YAML frontmatter block".to_string(),
                        });
                    }
                }
            }
        }
    }

    Ok(FixSummary { fixed })
}

pub fn synthesize_adr_frontmatter(
    content: &str,
    file_name: &str,
    valid_enums: &[String],
) -> Option<String> {
    // 1. Determine ID
    let mut adr_id = None;
    let stem = file_name.strip_suffix(".md").unwrap_or(file_name);
    let first_part = stem.split('-').next().unwrap_or("");
    if !first_part.is_empty() && first_part.chars().all(|c| c.is_ascii_digit()) {
        adr_id = Some(format!("ADR-{}", first_part));
    }

    // 2. Parse Title & refine ID from first heading
    let mut title = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(h1) = trimmed.strip_prefix("# ") {
            let heading = h1.trim();
            if heading.to_lowercase().starts_with("adr-") {
                if let Some(colon_idx) = heading.find(':') {
                    let id_part = heading[..colon_idx].trim();
                    if adr_id.is_none() {
                        adr_id = Some(id_part.to_string());
                    }
                    title = Some(heading[colon_idx + 1..].trim().to_string());
                } else if let Some(dash_idx) = heading.find(" - ") {
                    let id_part = heading[..dash_idx].trim();
                    if adr_id.is_none() {
                        adr_id = Some(id_part.to_string());
                    }
                    title = Some(heading[dash_idx + 3..].trim().to_string());
                } else if let Some(dot_idx) = heading.find('.') {
                    let id_part = heading[..dot_idx].trim();
                    if adr_id.is_none() {
                        adr_id = Some(id_part.to_string());
                    }
                    title = Some(heading[dot_idx + 1..].trim().to_string());
                }
            } else if let Some((first_word, rest)) = heading.split_once(|c: char| c.is_whitespace())
            {
                let cleaned_first = first_word.trim_end_matches(&['.', ':', '-'][..]);
                if cleaned_first.chars().all(|c| c.is_ascii_digit()) && !cleaned_first.is_empty() {
                    if adr_id.is_none() {
                        adr_id = Some(format!("ADR-{}", cleaned_first));
                    }
                    title = Some(rest.trim().to_string());
                }
            } else if let Some(colon_idx) = heading.find(':') {
                let id_candidate = heading[..colon_idx].trim();
                if id_candidate.chars().all(|c| c.is_ascii_digit()) {
                    if adr_id.is_none() {
                        adr_id = Some(format!("ADR-{}", id_candidate));
                    }
                    title = Some(heading[colon_idx + 1..].trim().to_string());
                }
            } else if let Some(dot_idx) = heading.find('.') {
                let id_candidate = heading[..dot_idx].trim();
                if id_candidate.chars().all(|c| c.is_ascii_digit()) {
                    if adr_id.is_none() {
                        adr_id = Some(format!("ADR-{}", id_candidate));
                    }
                    title = Some(heading[dot_idx + 1..].trim().to_string());
                }
            } else if let Some(dash_idx) = heading.find(" - ") {
                let id_candidate = heading[..dash_idx].trim();
                if id_candidate.chars().all(|c| c.is_ascii_digit()) {
                    if adr_id.is_none() {
                        adr_id = Some(format!("ADR-{}", id_candidate));
                    }
                    title = Some(heading[dash_idx + 3..].trim().to_string());
                }
            }

            if title.is_none() {
                title = Some(heading.to_string());
            }
            break;
        }
    }

    let final_id = adr_id.unwrap_or_else(|| "ADR-0000".to_string());
    let final_title = title.unwrap_or_else(|| "Architecture Decision".to_string());

    // 3. Parse Status and superseded pointer
    let mut status: Option<String> = None;
    let mut superseded_by: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        let cleaned = trimmed.trim_start_matches(['-', '*', ' ', '\t']);
        let lower = cleaned.to_lowercase();
        let is_status_line = lower.starts_with("status:")
            || lower.starts_with("**status:**")
            || lower.starts_with("**status**:")
            || lower.starts_with("*status:*")
            || lower.starts_with("*status*:");

        if is_status_line {
            if let Some(colon_idx) = cleaned.find(':') {
                let raw_status = cleaned[colon_idx + 1..].trim().trim_matches('*').trim();
                let (canonical, pointer) = parse_status_and_pointer(raw_status, valid_enums, true);
                if let Some(c) = canonical {
                    status = Some(c);
                }
                if let Some(p) = pointer {
                    superseded_by = Some(p);
                }
                break;
            }
        }
    }

    // Check if notice or text explicitly mentions superseded when no explicit status line found
    if status.is_none() {
        for line in content.lines().take(20) {
            let lower = line.to_lowercase();
            if lower.contains("superseded by") {
                status = Some("superseded".to_string());
                if let Some(adr_idx) = lower.find("adr-") {
                    let after_adr = &line[adr_idx..];
                    let adr_token = after_adr
                        .split(|c: char| {
                            c.is_whitespace()
                                || c == ']'
                                || c == ')'
                                || c == '('
                                || c == ','
                                || c == '.'
                                || c == '`'
                        })
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !adr_token.is_empty() {
                        superseded_by = Some(adr_token.to_string());
                    }
                }
                break;
            }
        }
    }

    // If status is superseded but no pointer found in status line, check subsequent lines
    if status.as_deref() == Some("superseded") && superseded_by.is_none() {
        for line in content.lines() {
            let lower = line.to_lowercase();
            if let Some(idx) = lower.find("superseded by") {
                let after = &line[idx + 13..];
                if let Some(adr_idx) = after.to_lowercase().find("adr-") {
                    let after_adr = &after[adr_idx..];
                    let pointer = after_adr
                        .split(|c: char| {
                            c.is_whitespace()
                                || c == '.'
                                || c == ','
                                || c == ')'
                                || c == ']'
                                || c == '`'
                        })
                        .next()
                        .unwrap_or("");
                    if !pointer.is_empty() {
                        superseded_by = Some(pointer.trim().to_string());
                        break;
                    }
                }
            }
        }
    }

    let mut final_status = status.unwrap_or_else(|| "accepted".to_string());
    if final_status == "superseded" && superseded_by.is_none() {
        final_status = "deprecated".to_string();
    }

    // 4. Parse Date
    let mut date = "2026-01-01".to_string();
    for line in content.lines() {
        let trimmed = line.trim();
        let cleaned = trimmed.trim_start_matches(['-', '*', ' ', '\t']);
        let lower = cleaned.to_lowercase();
        let is_date_line = lower.starts_with("date:")
            || lower.starts_with("**date:**")
            || lower.starts_with("**date**:")
            || lower.starts_with("*date:*")
            || lower.starts_with("*date*:");

        if is_date_line {
            if let Some(colon_idx) = cleaned.find(':') {
                let raw_date = cleaned[colon_idx + 1..].trim();
                if let Some(d) = find_iso_date(raw_date) {
                    date = d;
                    break;
                }
            }
        }
    }

    if date == "2026-01-01" {
        for line in content.lines().take(25) {
            if let Some(d) = find_iso_date(line) {
                date = d;
                break;
            }
        }
    }

    let escaped_title = final_title.replace('"', "\\\"");

    let mut frontmatter = String::new();
    frontmatter.push_str("---\n");
    frontmatter.push_str(&format!("id: {}\n", final_id));
    frontmatter.push_str(&format!("title: \"{}\"\n", escaped_title));
    frontmatter.push_str(&format!("status: {}\n", final_status));
    frontmatter.push_str(&format!("date: {}\n", date));
    if let Some(pointer) = superseded_by {
        frontmatter.push_str(&format!(
            "superseded_by: \"{}\"\n",
            pointer.replace('"', "\\\"")
        ));
    }
    frontmatter.push_str("---\n\n");

    let new_content = format!("{}{}", frontmatter, content);
    Some(new_content)
}

fn find_iso_date(s: &str) -> Option<String> {
    for word in s.split(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == ')' || c == '`')
    {
        let w = word.trim();
        if w.len() == 10 {
            let bytes = w.as_bytes();
            if bytes[0..4].iter().all(|b| b.is_ascii_digit())
                && bytes[4] == b'-'
                && bytes[5..7].iter().all(|b| b.is_ascii_digit())
                && bytes[7] == b'-'
                && bytes[8..10].iter().all(|b| b.is_ascii_digit())
            {
                return Some(w.to_string());
            }
        }
    }
    None
}
