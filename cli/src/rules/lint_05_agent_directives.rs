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

pub fn is_begin_tag(s: &str) -> bool {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("<!--") {
        if let Some(inner) = rest.strip_suffix("-->") {
            return inner.trim().eq_ignore_ascii_case("BEGIN DOCGOV DIRECTIVES");
        }
    }
    false
}

pub fn is_end_tag(s: &str) -> bool {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("<!--") {
        if let Some(inner) = rest.strip_suffix("-->") {
            return inner.trim().eq_ignore_ascii_case("END DOCGOV DIRECTIVES");
        }
    }
    false
}

pub fn find_begin_delimiter_span(text: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let line_len = line.len();
        if is_begin_tag(line) {
            let trimmed = line.trim();
            let leading = line.find(trimmed).unwrap_or(0);
            return Some((offset + leading, offset + leading + trimmed.len()));
        }
        offset += line_len;
    }
    if let Some(idx) = text.find(DOCGOV_DIRECTIVES_BEGIN) {
        return Some((idx, idx + DOCGOV_DIRECTIVES_BEGIN.len()));
    }
    None
}

pub fn find_end_delimiter_span(text: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let line_len = line.len();
        if is_end_tag(line) {
            let trimmed = line.trim();
            let leading = line.find(trimmed).unwrap_or(0);
            return Some((offset + leading, offset + leading + trimmed.len()));
        }
        offset += line_len;
    }
    if let Some(idx) = text.find(DOCGOV_DIRECTIVES_END) {
        return Some((idx, idx + DOCGOV_DIRECTIVES_END.len()));
    }
    None
}

pub fn find_delimiter_line(text: &str, delimiter: &str) -> Option<usize> {
    if delimiter == DOCGOV_DIRECTIVES_BEGIN {
        find_begin_delimiter_span(text).map(|(s, _)| s)
    } else if delimiter == DOCGOV_DIRECTIVES_END {
        find_end_delimiter_span(text).map(|(s, _)| s)
    } else {
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
}

pub fn extract_directives_block(content: &str) -> Option<&str> {
    let (begin_start, _) = find_begin_delimiter_span(content).or_else(|| {
        content
            .find(DOCGOV_DIRECTIVES_BEGIN)
            .map(|i| (i, i + DOCGOV_DIRECTIVES_BEGIN.len()))
    })?;
    let (_, end_rel_end) = find_end_delimiter_span(&content[begin_start..]).or_else(|| {
        content[begin_start..]
            .find(DOCGOV_DIRECTIVES_END)
            .map(|i| (i, i + DOCGOV_DIRECTIVES_END.len()))
    })?;
    let end_idx = begin_start + end_rel_end;
    Some(&content[begin_start..end_idx])
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
    let detected_heading =
        extract_primary_heading(snippet).unwrap_or("## Documentation Governance Directives");
    let clean_heading = detected_heading
        .split("<!--")
        .next()
        .unwrap_or(detected_heading)
        .trim();

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

            let begin_opt = find_begin_delimiter_span(text);

            if let Some((begin_start, begin_end)) = begin_opt {
                let end_opt = find_end_delimiter_span(&text[begin_start..]);
                if let Some((_, end_rel_end)) = end_opt {
                    let end_idx = begin_start + end_rel_end;
                    let before = &text[..begin_start];
                    let after = &text[end_idx..];
                    let current_block = &text[begin_start..end_idx];
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
                    let remaining = &text[begin_end..];
                    let search_start = if let Some(h_idx) = remaining.find(clean_heading) {
                        h_idx + clean_heading.len()
                    } else {
                        0
                    };
                    let end_idx = find_next_major_heading(&remaining[search_start..])
                        .map(|offset| begin_end + search_start + offset)
                        .unwrap_or(text.len());
                    let before = text[..begin_start].trim_end();
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

            // Case: Delimiters missing, but directive heading or anchor exists
            let head_opt = if let Some(a_idx) = text.find("@docgov-anchor") {
                let line_start = text[..a_idx].rfind('\n').map(|i| i + 1).unwrap_or(0);
                Some((line_start, "@docgov-anchor".len()))
            } else {
                text.find(clean_heading)
                    .map(|h_idx| (h_idx, clean_heading.len()))
            };

            if let Some((head_idx, match_len)) = head_opt {
                let after_head = &text[head_idx + match_len..];
                let end_idx = if let Some((_, end_rel_end)) = find_end_delimiter_span(after_head) {
                    head_idx + match_len + end_rel_end
                } else {
                    find_next_major_heading(after_head)
                        .map(|offset| head_idx + match_len + offset)
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
                    .with_suggestion("Run `docgov fix` (or `docgov init`) to create it with the non-invasive docgov directives block."),
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

            let begin_opt = find_begin_delimiter_span(&content);
            let end_opt = begin_opt.and_then(|(b_start, _)| {
                find_end_delimiter_span(&content[b_start..])
                    .map(|(_, e_rel_end)| b_start + e_rel_end)
            });

            match (begin_opt, end_opt) {
                (Some((begin_idx, _)), Some(end_idx)) => {
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
                                        .with_suggestion("Run `docgov fix` (or `docgov update`) to re-synchronize directives with upstream lockfile."),
                                    );
                                }
                            }
                        }
                    }
                }
                (Some((begin_idx, _)), None) => {
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
                        .with_suggestion("Run `docgov fix` to restore the delimiter comments."),
                    );
                }
                _ => {
                    let line_num = content.lines().count().max(1);
                    let looks_corrupted = is_end_tag(&content)
                        || content.contains(DOCGOV_DIRECTIVES_END)
                        || content.contains("@docgov-anchor")
                        || content.contains("Documentation Governance Directives")
                        || content.contains("Hard Constraints (Zero-Tolerance Invariants)")
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
                            .with_suggestion("Run `docgov fix` to restore the boundary comments."),
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
                            .with_suggestion("Run `docgov fix` (or `docgov init`) to insert the non-invasive docgov directives block without overwriting existing instructions."),
                        );
                    }
                }
            }
        }

        Ok(diagnostics)
    }
}
