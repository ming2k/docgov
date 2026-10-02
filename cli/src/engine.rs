use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::ast::MarkdownDocument;
use crate::config::Config;
use crate::diagnostics::Diagnostic;
use crate::rules::{all_rules, LintContext};

pub struct LintEngine {
    pub workspace_root: PathBuf,
    pub config: Config,
    pub config_path: Option<PathBuf>,
}

impl LintEngine {
    pub fn new<P: AsRef<Path>>(workspace_root: P) -> Result<Self> {
        let root = workspace_root.as_ref().to_path_buf();
        let (config, config_path) = Config::load_from_dir(&root)?;
        Ok(Self {
            workspace_root: root,
            config,
            config_path,
        })
    }

    pub fn with_config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    pub fn run_lint(&self, changed_files: Option<&[PathBuf]>) -> Result<Vec<Diagnostic>> {
        // 1. Collect root directory entries
        let root_files = self.collect_root_files()?;

        // 2. Discover and parse all markdown documents
        let markdown_docs = self.discover_markdown_docs()?;

        // 3. Build LintContext
        let ctx = LintContext {
            workspace_root: &self.workspace_root,
            config: &self.config,
            root_files: &root_files,
            markdown_docs: &markdown_docs,
            changed_files,
        };

        // 4. Run rules
        let mut diagnostics = Vec::new();
        for rule in all_rules() {
            let res = rule.check(&ctx)?;
            diagnostics.extend(res);
        }

        // 5. Sort diagnostics by file_path and line
        diagnostics.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then(a.line.cmp(&b.line))
                .then(a.column.cmp(&b.column))
        });

        Ok(diagnostics)
    }

    fn collect_root_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.workspace_root) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_file() {
                        let path = entry.path();
                        if let Ok(rel) = path.strip_prefix(&self.workspace_root) {
                            files.push(rel.to_path_buf());
                        }
                    }
                }
            }
        }
        Ok(files)
    }

    fn discover_markdown_docs(&self) -> Result<HashMap<PathBuf, MarkdownDocument>> {
        let mut docs = HashMap::new();

        let walker = WalkDir::new(&self.workspace_root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                if e.depth() == 0 {
                    return true;
                }
                let name = e.file_name().to_string_lossy();
                // Ignore build and vcs directories
                !name.starts_with('.')
            });

        for entry in walker.flatten() {
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let Ok(rel) = path.strip_prefix(&self.workspace_root) else {
                continue;
            };

            let is_hidden = rel.components().any(|c| {
                let s = c.as_os_str().to_string_lossy();
                s.starts_with('.') && s != "."
            });
            if is_hidden {
                continue;
            }

            let is_target_or_build = rel.components().any(|c| {
                let s = c.as_os_str().to_string_lossy();
                s == "target" || s == "node_modules" || s == "dist" || s == "build"
            });
            if is_target_or_build {
                continue;
            }

            if let Some(ext) = path.extension() {
                if ext.to_string_lossy().eq_ignore_ascii_case("md") {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        let doc = MarkdownDocument::parse(&content);
                        docs.insert(rel.to_path_buf(), doc);
                    }
                }
            }
        }

        Ok(docs)
    }
}
