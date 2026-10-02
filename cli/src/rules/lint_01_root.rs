use crate::diagnostics::Diagnostic;
use crate::rules::{LintContext, Rule};
use anyhow::Result;

pub struct RootSanitizerRule;

impl Rule for RootSanitizerRule {
    fn id(&self) -> &'static str {
        "INV-LINT-01"
    }

    fn description(&self) -> &'static str {
        "Location Sanitization: Prohibit unapproved Markdown files at repository root"
    }

    fn check(&self, ctx: &LintContext) -> Result<Vec<Diagnostic>> {
        let mut diagnostics = Vec::new();

        if !ctx.config.root_sanitization.enforce {
            return Ok(diagnostics);
        }

        let allowed: std::collections::HashSet<&str> = ctx
            .config
            .root_sanitization
            .allowed_markdown
            .iter()
            .map(|s| s.as_str())
            .collect();

        for file in ctx.root_files {
            if let Some(ext) = file.extension() {
                if ext.to_string_lossy().eq_ignore_ascii_case("md") {
                    let file_name = file.file_name().unwrap_or_default().to_string_lossy();
                    if !allowed.contains(file_name.as_ref()) {
                        diagnostics.push(
                            Diagnostic::error(
                                self.id(),
                                format!(
                                    "Unapproved Markdown file at repository root: '{}'",
                                    file_name
                                ),
                                file,
                            )
                            .with_suggestion(format!(
                                "Move to 'docs/' or add to 'root_sanitization.allowed_markdown' in .docgov.yml. Allowed: {}",
                                ctx.config.root_sanitization.allowed_markdown.join(", ")
                            )),
                        );
                    }
                }
            }
        }

        Ok(diagnostics)
    }
}
