use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::ast::MarkdownDocument;
use crate::config::Config;
use crate::diagnostics::Diagnostic;

pub mod lint_01_root;
pub mod lint_02_firewall;
pub mod lint_03_frontmatter;
pub mod lint_04_trigger;
pub mod lint_05_agent_directives;

pub struct LintContext<'a> {
    pub workspace_root: &'a PathBuf,
    pub config: &'a Config,
    pub root_files: &'a [PathBuf],
    pub markdown_docs: &'a HashMap<PathBuf, MarkdownDocument>,
    pub changed_files: Option<&'a [PathBuf]>,
}

pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn check(&self, ctx: &LintContext) -> Result<Vec<Diagnostic>>;
}

pub fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(lint_01_root::RootSanitizerRule),
        Box::new(lint_02_firewall::FirewallRule),
        Box::new(lint_03_frontmatter::FrontmatterSchemaRule),
        Box::new(lint_04_trigger::GitSyncTriggerRule),
        Box::new(lint_05_agent_directives::AgentDirectivesRule),
    ]
}
