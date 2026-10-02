use crate::diagnostics::Diagnostic;
use crate::rules::{LintContext, Rule};
use crate::utils::matches_glob;
use anyhow::Result;

pub struct GitSyncTriggerRule;

impl Rule for GitSyncTriggerRule {
    fn id(&self) -> &'static str {
        "INV-LINT-04"
    }

    fn description(&self) -> &'static str {
        "Code-Doc Synchronization Trigger: Monitored code changes require synchronized doc updates"
    }

    fn check(&self, ctx: &LintContext) -> Result<Vec<Diagnostic>> {
        let mut diagnostics = Vec::new();

        let changed_files = match ctx.changed_files {
            Some(files) if !files.is_empty() => files,
            _ => return Ok(diagnostics), // No git diff to analyze, skip
        };

        for trigger in &ctx.config.triggers {
            let matched_source = changed_files
                .iter()
                .find(|file| matches_glob(file, &trigger.watch));

            if let Some(source_file) = matched_source {
                let doc_updated = changed_files.iter().any(|file| {
                    trigger
                        .require_update
                        .iter()
                        .any(|pattern| matches_glob(file, pattern))
                });

                if !doc_updated {
                    let requirement_desc = trigger.require_update.join("' or '");
                    let msg = trigger.message.clone().unwrap_or_else(|| {
                        format!(
                            "Monitored path '{}' modified, but none of the required documentation surfaces ('{}') were updated",
                            trigger.watch, requirement_desc
                        )
                    });

                    diagnostics.push(
                        Diagnostic::error(self.id(), msg, source_file).with_suggestion(format!(
                            "Update matching documentation in '{}' in the same change.",
                            requirement_desc
                        )),
                    );
                }
            }
        }

        Ok(diagnostics)
    }
}
