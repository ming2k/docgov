use crate::diagnostics::Diagnostic;
use crate::lockfile::{compute_sha256, DocgovLock};
use crate::rules::{LintContext, Rule};
use anyhow::Result;

pub const DOCGOV_DIRECTIVES_BEGIN: &str = "<!-- BEGIN DOCGOV DIRECTIVES -->";
pub const DOCGOV_DIRECTIVES_END: &str = "<!-- END DOCGOV DIRECTIVES -->";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchAction {
    Created,
    Appended,
    Updated,
    Unchanged,
}

pub fn find_delimiter_line(text: &str, delimiter: &str) -> Option<usize> {
    for (i, _) in text.match_indices(delimiter) {
        let is_line_start = i == 0 || text[..i].ends_with('\n');
        let after_idx = i + delimiter.len();
        let is_line_end = after_idx >= text.len()
            || text[after_idx..].starts_with('\n')
            || text[after_idx..].starts_with("\r\n");
        if is_line_start && is_line_end {
            return Some(i);
        }
    }
    None
}

pub fn extract_directives_block(content: &str) -> Option<&str> {
    let begin_idx = find_delimiter_line(content, DOCGOV_DIRECTIVES_BEGIN)
        .or_else(|| content.find(DOCGOV_DIRECTIVES_BEGIN))?;
    let end_rel = find_delimiter_line(&content[begin_idx..], DOCGOV_DIRECTIVES_END)
        .or_else(|| content[begin_idx..].find(DOCGOV_DIRECTIVES_END))?;
    let end_idx = begin_idx + end_rel + DOCGOV_DIRECTIVES_END.len();
    Some(&content[begin_idx..end_idx])
}

fn find_next_major_heading(text: &str) -> Option<usize> {
    for (i, _) in text.match_indices("\n#") {
        let rest = &text[i + 1..];
        if rest.starts_with("# ") || (rest.starts_with("## ") && !rest.starts_with("### ")) {
            return Some(i + 1);
        }
    }
    None
}

/// Extract primary Markdown heading from the spec snippet (e.g. "## Documentation Governance Directives")
pub fn extract_primary_heading(snippet: &str) -> Option<&str> {
    for line in snippet.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") || trimmed.starts_with("# ") {
            return Some(trimmed);
        }
    }
    None
}

pub fn patch_agent_directives(
    existing_content: Option<&str>,
    snippet: &str,
) -> (String, PatchAction) {
    let snippet = snippet.trim();
    let detected_heading = extract_primary_heading(snippet).unwrap_or("## Documentation Governance Directives");

    match existing_content {
        None => {
            let content = format!("{}\n", snippet);
            (content, PatchAction::Created)
        }
        Some(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                let content = format!("{}\n", snippet);
                return (content, PatchAction::Created);
            }

            let begin_opt = find_delimiter_line(text, DOCGOV_DIRECTIVES_BEGIN)
                .or_else(|| text.find(DOCGOV_DIRECTIVES_BEGIN));

            if let Some(begin_idx) = begin_opt {
                let end_opt = find_delimiter_line(&text[begin_idx..], DOCGOV_DIRECTIVES_END)
                    .or_else(|| text[begin_idx..].find(DOCGOV_DIRECTIVES_END));
                if let Some(end_rel) = end_opt {
                    let end_idx = begin_idx + end_rel + DOCGOV_DIRECTIVES_END.len();
                    let before = &text[..begin_idx];
                    let after = &text[end_idx..];
                    let current_block = &text[begin_idx..end_idx];
                    if current_block.trim() == snippet {
                        return (text.to_string(), PatchAction::Unchanged);
                    }
                    let mut result = String::new();
                    result.push_str(before);
                    result.push_str(snippet);
                    result.push_str(after);
                    return (result, PatchAction::Updated);
                } else {
                    // Delimiter begin exists, but closing delimiter was stripped
                    let remaining = &text[begin_idx + DOCGOV_DIRECTIVES_BEGIN.len()..];
                    let search_start = if let Some(h_idx) = remaining.find(detected_heading) {
                        h_idx + detected_heading.len()
                    } else {
                        0
                    };
                    let end_idx = find_next_major_heading(&remaining[search_start..])
                        .map(|offset| {
                            begin_idx + DOCGOV_DIRECTIVES_BEGIN.len() + search_start + offset
                        })
                        .unwrap_or(text.len());
                    let before = text[..begin_idx].trim_end();
                    let after = text[end_idx..].trim_start_matches('\n');
                    let mut result = String::new();
                    if !before.is_empty() {
                        result.push_str(before);
                        result.push_str("\n\n");
                    }
                    result.push_str(snippet);
                    if !after.is_empty() {
                        result.push_str("\n\n");
                        result.push_str(after);
                    }
                    result.push('\n');
                    return (result, PatchAction::Updated);
                }
            }

            // Case: Delimiters missing, but directive heading exists (AI stripped delimiters/reorganized)
            if let Some(head_idx) = text.find(detected_heading) {
                let after_head = &text[head_idx + detected_heading.len()..];
                let end_idx = if let Some(end_rel) =
                    find_delimiter_line(after_head, DOCGOV_DIRECTIVES_END)
                        .or_else(|| after_head.find(DOCGOV_DIRECTIVES_END))
                {
                    head_idx + detected_heading.len() + end_rel + DOCGOV_DIRECTIVES_END.len()
                } else {
                    find_next_major_heading(after_head)
                        .map(|offset| head_idx + detected_heading.len() + offset)
                        .unwrap_or(text.len())
                };

                let before = text[..head_idx].trim_end();
                let after = text[end_idx..].trim_start_matches('\n');
                let mut result = String::new();
                if !before.is_empty() {
                    result.push_str(before);
                    result.push_str("\n\n");
                }
                result.push_str(snippet);
                if !after.is_empty() {
                    result.push_str("\n\n");
                    result.push_str(after);
                }
                result.push('\n');
                return (result, PatchAction::Updated);
            }

            // Marker does not exist: append non-invasively
            let mut result = text.trim_end().to_string();
            result.push_str("\n\n");
            result.push_str(snippet);
            result.push('\n');
            (result, PatchAction::Appended)
        }
    }
}

pub struct AgentDirectivesRule;

impl Rule for AgentDirectivesRule {
    fn id(&self) -> &'static str {
        "INV-LINT-05"
    }

    fn description(&self) -> &'static str {
        "Agent Directives Binding: AI assistant configuration must contain docgov anchor directives"
    }

    fn check(&self, ctx: &LintContext) -> Result<Vec<Diagnostic>> {
        let mut diagnostics = Vec::new();

        if !ctx.config.agent_directives.enforce {
            return Ok(diagnostics);
        }

        let lock = DocgovLock::load_from_dir(ctx.workspace_root).ok().flatten();

        for target in &ctx.config.agent_directives.targets {
            let target_path = ctx.workspace_root.join(target);
            let rel_target = std::path::PathBuf::from(target);

            if !target_path.exists() {
                diagnostics.push(
                    Diagnostic::error(
                        self.id(),
                        format!("Missing agent directives configuration file '{}'", target),
                        rel_target,
                    )
                    .with_suggestion("Run `docgov init` to create it with the non-invasive docgov directives block."),
                );
                continue;
            }

            let content = match std::fs::read_to_string(&target_path) {
                Ok(c) => c,
                Err(_) => {
                    diagnostics.push(Diagnostic::error(
                        self.id(),
                        format!("Failed to read agent configuration file '{}'", target),
                        rel_target,
                    ));
                    continue;
                }
            };

            let begin_opt = find_delimiter_line(&content, DOCGOV_DIRECTIVES_BEGIN)
                .or_else(|| content.find(DOCGOV_DIRECTIVES_BEGIN));
            let end_opt = begin_opt.and_then(|b| {
                find_delimiter_line(&content[b..], DOCGOV_DIRECTIVES_END)
                    .or_else(|| content[b..].find(DOCGOV_DIRECTIVES_END))
                    .map(|e| b + e + DOCGOV_DIRECTIVES_END.len())
            });

            match (begin_opt, end_opt) {
                (Some(begin_idx), Some(end_idx)) => {
                    // Check against lockfile if present
                    if let Some(ref lockfile) = lock {
                        if let Some(ref artifact) = lockfile.artifacts.agent_directives {
                            if artifact.target == *target {
                                let block_slice = content[begin_idx..end_idx].trim();
                                let actual_hash = compute_sha256(block_slice);
                                if actual_hash != artifact.hash {
                                    let line_num = content[..begin_idx].lines().count().max(1);
                                    diagnostics.push(
                                        Diagnostic::warning(
                                            self.id(),
                                            format!(
                                                "Directives block in '{}' drifted from .docgov.lock (expected {}, got {})",
                                                target, artifact.hash, actual_hash
                                            ),
                                            rel_target.clone(),
                                        )
                                        .with_location(line_num, 1)
                                        .with_suggestion("Run `docgov init` to re-synchronize directives with upstream lockfile."),
                                    );
                                }
                            }
                        }
                    }
                }
                (Some(begin_idx), None) => {
                    let line_num = content[..begin_idx].lines().count().max(1);
                    diagnostics.push(
                        Diagnostic::error(
                            self.id(),
                            format!(
                                "Directives block in '{}' is missing closing delimiter tag '{}'. AI assistants and editors must preserve boundary comments.",
                                target, DOCGOV_DIRECTIVES_END
                            ),
                            rel_target,
                        )
                        .with_location(line_num, 1)
                        .with_suggestion("Run `docgov update` or `docgov init` to restore the delimiter comments."),
                    );
                }
                _ => {
                    let line_num = content.lines().count().max(1);
                    let looks_corrupted = content.contains(DOCGOV_DIRECTIVES_END)
                        || content.contains("Documentation Governance Directives")
                        || content.contains("Machine Invariants (Pre-Submit Checklist)");
                    if looks_corrupted {
                        diagnostics.push(
                            Diagnostic::error(
                                self.id(),
                                format!(
                                    "Docgov anchor boundary comments were stripped or corrupted in '{}'. AI assistants must preserve '{}' and '{}' verbatim without reorganizing.",
                                    target, DOCGOV_DIRECTIVES_BEGIN, DOCGOV_DIRECTIVES_END
                                ),
                                rel_target,
                            )
                            .with_location(line_num, 1)
                            .with_suggestion("Run `docgov update` or `docgov init` to restore the boundary comments."),
                        );
                    } else {
                        diagnostics.push(
                            Diagnostic::error(
                                self.id(),
                                format!(
                                    "Missing docgov anchor directives block in '{}'",
                                    target
                                ),
                                rel_target,
                            )
                            .with_location(line_num, 1)
                            .with_suggestion("Run `docgov init` to insert the non-invasive docgov directives block without overwriting existing instructions."),
                        );
                    }
                }
            }
        }

        Ok(diagnostics)
    }
}
