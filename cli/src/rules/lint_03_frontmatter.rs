use crate::diagnostics::Diagnostic;
use crate::rules::{LintContext, Rule};
use crate::utils::matches_glob;
use anyhow::Result;
use serde_yaml::Value;

pub struct FrontmatterSchemaRule;

/// Extract canonical status and potential superseded pointer from a raw status string.
pub fn parse_status_and_pointer(
    raw: &str,
    valid_enums: &[String],
    tolerant: bool,
) -> (Option<String>, Option<String>) {
    let raw_trimmed = raw.trim();

    // 1. Exact match (case-insensitive)
    for valid in valid_enums {
        if valid.eq_ignore_ascii_case(raw_trimmed) {
            return (Some(valid.to_lowercase()), None);
        }
    }

    if !tolerant {
        return (None, None);
    }

    // 2. Tolerant token extraction:
    // Split on whitespace or common punctuation (em dash, hyphen, parens, brackets, colon)
    let token = raw_trimmed
        .split(|c: char| {
            c.is_whitespace()
                || c == '—'
                || c == '-'
                || c == '('
                || c == ')'
                || c == '['
                || c == ']'
                || c == ':'
        })
        .find(|s| !s.is_empty())
        .unwrap_or("");

    let matched_enum = valid_enums
        .iter()
        .find(|valid| valid.eq_ignore_ascii_case(token))
        .map(|s| s.to_lowercase());

    // 3. Pointer extraction if superseded
    let mut pointer = None;
    if matched_enum.as_deref() == Some("superseded") {
        let lower = raw_trimmed.to_lowercase();
        if let Some(adr_idx) = lower.find("adr-") {
            let after_adr = &raw_trimmed[adr_idx..];
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
                pointer = Some(adr_token.to_string());
            }
        } else if let Some(idx) = lower.find("by ") {
            let after_by = raw_trimmed[idx + 3..].trim();
            let token = after_by
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
                .trim_matches(&['[', ']', '(', ')', '`'][..]);
            if !token.is_empty() {
                pointer = Some(token.to_string());
            }
        }
    }

    (matched_enum, pointer)
}

impl Rule for FrontmatterSchemaRule {
    fn id(&self) -> &'static str {
        "INV-LINT-03"
    }

    fn description(&self) -> &'static str {
        "Frontmatter Schema & Lifecycle Integrity: ADRs must declare valid frontmatter and status"
    }

    fn check(&self, ctx: &LintContext) -> Result<Vec<Diagnostic>> {
        if !ctx.config.architecture.enforce || !ctx.config.architecture.require_frontmatter.enforce
        {
            return Ok(Vec::new());
        }

        let mut diagnostics = Vec::new();
        let adr_glob = format!(
            "{}/**/*.md",
            ctx.config.architecture.adr_path.trim_end_matches('/')
        );

        'doc_loop: for (rel_path, doc) in ctx.markdown_docs {
            if !matches_glob(rel_path, &adr_glob) {
                continue;
            }

            // Exclude pattern check
            for exclude_glob in &ctx.config.architecture.require_frontmatter.exclude {
                if matches_glob(rel_path, exclude_glob) {
                    continue 'doc_loop;
                }
            }

            let file_name = rel_path.file_name().unwrap_or_default().to_string_lossy();
            if file_name == "index.md" || file_name == "README.md" || file_name == "template.md" {
                continue;
            }

            // 1. Must have Frontmatter
            let fm = match &doc.frontmatter {
                Some(f) => f,
                None => {
                    diagnostics.push(
                        Diagnostic::error(
                            self.id(),
                            "Missing YAML frontmatter in ADR",
                            rel_path,
                        )
                        .with_location(1, 1)
                        .with_suggestion("Add frontmatter block (---) with 'id', 'title', 'status', and 'date'."),
                    );
                    continue;
                }
            };

            // 2. Frontmatter must be a mapping
            let map = match &fm.value {
                Value::Mapping(m) => m,
                _ => {
                    diagnostics.push(
                        Diagnostic::error(
                            self.id(),
                            "Frontmatter must be a YAML key-value mapping",
                            rel_path,
                        )
                        .with_location(1, 1),
                    );
                    continue;
                }
            };

            // 3. Mandatory fields check
            for field in &ctx.config.architecture.require_frontmatter.mandatory_fields {
                if !map.contains_key(field.as_str()) || map.get(field.as_str()).unwrap().is_null() {
                    diagnostics.push(
                        Diagnostic::error(
                            self.id(),
                            format!("Missing mandatory frontmatter field: '{}'", field),
                            rel_path,
                        )
                        .with_location(fm.start_line, 1)
                        .with_suggestion(format!("Declare '{}' in the frontmatter header.", field)),
                    );
                }
            }

            // 4. Status enum check
            if let Some(status_val) = map.get("status") {
                let status_raw = match status_val {
                    Value::String(s) => s.as_str(),
                    _ => "",
                };
                let valid_enum = &ctx.config.architecture.require_frontmatter.status_enum;
                let tolerant = ctx.config.architecture.require_frontmatter.tolerant_status;

                let (canonical_status, extracted_pointer) =
                    parse_status_and_pointer(status_raw, valid_enum, tolerant);

                match canonical_status {
                    Some(canonical) => {
                        // 5. If superseded, require superseded_by
                        if canonical == "superseded" {
                            let has_valid_superseded_by = match map.get("superseded_by") {
                                Some(val) => match val {
                                    Value::String(s) => !s.trim().is_empty(),
                                    _ => false,
                                },
                                None => extracted_pointer.is_some(),
                            };

                            if !has_valid_superseded_by {
                                diagnostics.push(
                                    Diagnostic::error(
                                        self.id(),
                                        "ADR marked as 'superseded' must provide a valid 'superseded_by' pointer (e.g. 'ADR-0042')",
                                        rel_path,
                                    )
                                    .with_location(fm.start_line, 1)
                                    .with_suggestion("Add 'superseded_by: ADR-NNNN' to frontmatter."),
                                );
                            }
                        }
                    }
                    None => {
                        diagnostics.push(
                            Diagnostic::error(
                                self.id(),
                                format!(
                                    "Invalid status '{}'. Must be one of: {}",
                                    status_raw,
                                    valid_enum.join(", ")
                                ),
                                rel_path,
                            )
                            .with_location(fm.start_line, 1),
                        );
                    }
                }
            }
        }

        Ok(diagnostics)
    }
}
