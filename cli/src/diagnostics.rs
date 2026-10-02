use colored::*;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub file_path: PathBuf,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub snippet: Option<String>,
    pub suggestion: Option<String>,
}

impl Diagnostic {
    pub fn error(
        rule_id: impl Into<String>,
        message: impl Into<String>,
        file_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            rule_id: rule_id.into(),
            severity: Severity::Error,
            message: message.into(),
            file_path: file_path.into(),
            line: None,
            column: None,
            snippet: None,
            suggestion: None,
        }
    }

    pub fn warning(
        rule_id: impl Into<String>,
        message: impl Into<String>,
        file_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            rule_id: rule_id.into(),
            severity: Severity::Warning,
            message: message.into(),
            file_path: file_path.into(),
            line: None,
            column: None,
            snippet: None,
            suggestion: None,
        }
    }

    pub fn with_location(mut self, line: usize, column: usize) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    pub fn with_snippet(mut self, snippet: impl Into<String>) -> Self {
        self.snippet = Some(snippet.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Format for terminal with rich ANSI coloring
    pub fn render_terminal(&self) -> String {
        let tag = match self.severity {
            Severity::Error => format!("[{}]", self.rule_id).red().bold(),
            Severity::Warning => format!("[{}]", self.rule_id).yellow().bold(),
            Severity::Note => format!("[{}]", self.rule_id).blue().bold(),
        };

        let loc = match (self.line, self.column) {
            (Some(l), Some(c)) => format!("{}:{}:{}", self.file_path.display(), l, c),
            (Some(l), None) => format!("{}:{}", self.file_path.display(), l),
            (None, _) => format!("{}", self.file_path.display()),
        };

        let mut out = format!("  {} {}\n   ╭─[ {} ]\n", "×".red().bold(), tag, loc.cyan());

        if let Some(snippet) = &self.snippet {
            let l_str = self
                .line
                .map(|l| l.to_string())
                .unwrap_or_else(|| "·".to_string());
            out.push_str(&format!("{:>3} │ {}\n", l_str.dimmed(), snippet));
            out.push_str(&format!("   · {}\n", format!("╰── {}", self.message).red()));
        } else {
            out.push_str(&format!("   · {}\n", self.message));
        }

        if let Some(suggestion) = &self.suggestion {
            out.push_str(&format!(
                "   ╰────\n  {}: {}\n",
                "help".green().bold(),
                suggestion
            ));
        } else {
            out.push_str("   ╰────\n");
        }

        out
    }

    /// Format as GitHub Action workflow command
    pub fn render_github(&self) -> String {
        let level = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "notice",
        };

        let mut meta = format!("file={}", self.file_path.display());
        if let Some(l) = self.line {
            meta.push_str(&format!(",line={}", l));
        }
        if let Some(c) = self.column {
            meta.push_str(&format!(",col={}", c));
        }
        meta.push_str(&format!(",title=[{}]", self.rule_id));

        format!("::{} {}::{}", level, meta, self.message)
    }
}
