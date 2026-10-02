use crate::diagnostics::Diagnostic;
use crate::rules::{LintContext, Rule};
use crate::utils::{matches_glob, resolve_relative_link};
use anyhow::Result;

pub struct FirewallRule;

impl Rule for FirewallRule {
    fn id(&self) -> &'static str {
        "INV-LINT-02"
    }

    fn description(&self) -> &'static str {
        "Contributor Firewall: Public docs must not link into internal spaces (docs/dev/**)"
    }

    fn check(&self, ctx: &LintContext) -> Result<Vec<Diagnostic>> {
        if !ctx.config.firewall.enforce {
            return Ok(Vec::new());
        }

        let mut diagnostics = Vec::new();

        for (rel_path, doc) in ctx.markdown_docs {
            // Check if document is in a public surface
            let is_public = ctx
                .config
                .firewall
                .public_surfaces
                .iter()
                .any(|pattern| matches_glob(rel_path, pattern));

            if !is_public {
                continue;
            }

            for link in &doc.links {
                if let Some(target) = resolve_relative_link(rel_path, &link.destination) {
                    let targets_internal = ctx
                        .config
                        .firewall
                        .internal_surfaces
                        .iter()
                        .any(|pattern| matches_glob(&target, pattern));

                    if targets_internal {
                        diagnostics.push(
                            Diagnostic::error(
                                self.id(),
                                format!(
                                    "Public document contains relative link into internal surface '{}'",
                                    target.display()
                                ),
                                rel_path,
                            )
                            .with_location(link.line, link.column)
                            .with_snippet(&link.line_snippet)
                            .with_suggestion(
                                "Move shared concepts into public documentation (e.g. 'docs/explanation/') or remove internal link.",
                            ),
                        );
                    }
                }
            }
        }

        Ok(diagnostics)
    }
}
