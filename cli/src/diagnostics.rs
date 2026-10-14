use serde::Serialize;
use std::path::PathBuf;

use crate::output;

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

    /// Render the diagnostic for a terminal.
    ///
    /// The layout is deliberately uniform across all four shapes (with/without
    /// snippet, with/without suggestion) so that a log is scannable:
    ///
    /// ```text
    ///   × [INV-LINT-02]  docs/tutorials/quick.md:3:5
    ///   3 │ See [dev](../dev/setup.md).
    ///   · Public document contains relative link into internal surface 'docs/dev/setup.md'
    ///   help: Move shared concepts into public documentation (e.g. 'docs/explanation/').
    /// ```
    ///
    /// The severity glyph (`×` / `!` / `i`) is distinct per severity, and the rule id
    /// and location always share the header line. Decoration is emitted only when
    /// color/glyph settings allow it; see [`crate::output`].
    pub fn render_terminal(&self) -> String {
        let level = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        };
        let tag = format!("{}[{}]", level, self.rule_id);
        let colored_tag = output::paint(&tag, output::severity_color(&self.severity));
        let loc = output::cyan(&self.location_string());
        let pipe_str = output::pipe();

        let mut out = format!("{}: {}\n", colored_tag, self.message);
        out.push_str(&format!("  {} {}\n", output::dimmed("-->"), loc));

        if let Some(snippet) = &self.snippet {
            let l_str = self
                .line
                .map(|l| l.to_string())
                .unwrap_or_else(|| "-".to_string());
            let pad = l_str.len().max(2);
            out.push_str(&format!(
                "{:>width$} {}\n",
                "",
                output::dimmed(pipe_str),
                width = pad
            ));
            out.push_str(&format!(
                "{:>width$} {} {}\n",
                output::dimmed(&l_str),
                output::dimmed(pipe_str),
                snippet,
                width = pad
            ));
            out.push_str(&format!(
                "{:>width$} {}\n",
                "",
                output::dimmed(pipe_str),
                width = pad
            ));
        }

        if let Some(suggestion) = &self.suggestion {
            out.push_str(&format!(
                "   = {}: {}\n",
                output::green("help"),
                suggestion
            ));
        }

        out
    }

    /// `path:line:col`, degrading gracefully when location data is absent.
    ///
    /// This is the single source of truth for the location fragment, shared by the
    /// text and GitHub renderers so the two can never drift apart.
    pub fn location_string(&self) -> String {
        match (self.line, self.column) {
            (Some(l), Some(c)) => format!("{}:{}:{}", self.file_path.display(), l, c),
            (Some(l), None) => format!("{}:{}", self.file_path.display(), l),
            (None, _) => self.file_path.display().to_string(),
        }
    }

    /// Format as a GitHub Action workflow command.
    ///
    /// Emits the canonical `::error file=...,line=...,col=...,title=...::message` form.
    /// The message is escaped for the workflow-command grammar so that a diagnostic
    /// containing `%`, `\r`, or `\n` cannot truncate or forge the annotation.
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

        format!(
            "::{} {}::{}",
            level,
            meta,
            escape_github_message(&self.message)
        )
    }
}

/// Escape a payload for the GitHub Actions workflow-command grammar.
///
/// The runner parses `::`-delimited commands line by line, so unescaped `%`, CR, or LF
/// in a diagnostic message would corrupt the stream.
fn escape_github_message(message: &str) -> String {
    message
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// Machine-readable envelope emitted for `--format=json`.
///
/// A bare array (the pre-contract shape) leaves consumers with no way to distinguish
/// "no findings" from "engine failed to start", and no way to branch on schema
/// evolution. The envelope carries the protocol version and a severity tally so a CI
/// job can gate on `summary.errors` without re-deriving it.
///
/// Timing is intentionally absent: this document is meant to be byte-reproducible for
/// a given workspace, which makes it diffable and snapshot-testable.
#[derive(Debug, Serialize)]
pub struct DiagnosticReport<'a> {
    pub schema_version: u32,
    pub protocol_version: String,
    pub summary: ReportSummary,
    pub diagnostics: &'a [Diagnostic],
}

/// Severity tally for a [`DiagnosticReport`].
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ReportSummary {
    pub errors: usize,
    pub warnings: usize,
    pub notes: usize,
    pub total: usize,
    /// Findings hidden by a baseline file, reported so "check passed" is never
    /// mistaken for "repository is clean".
    pub suppressed: usize,
}

impl ReportSummary {
    pub fn tally(diagnostics: &[Diagnostic], suppressed: usize) -> Self {
        let count = |want: Severity| diagnostics.iter().filter(|d| d.severity == want).count();
        Self {
            errors: count(Severity::Error),
            warnings: count(Severity::Warning),
            notes: count(Severity::Note),
            total: diagnostics.len(),
            suppressed,
        }
    }
}

/// The schema version of the `--format=json` document. Bump on any breaking change.
pub const JSON_SCHEMA_VERSION: u32 = 1;
