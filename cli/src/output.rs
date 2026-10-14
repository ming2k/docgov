//! Unified CLI presentation layer.
//!
//! Every human-readable or machine-readable byte the engine emits flows through this
//! module. Centralizing it guarantees a single, enforceable output contract:
//!
//! 1. **One glyph per semantic role.** Progress/result lines use a fixed glyph set
//!    (`✔ + ~ - ! × i`); diagnostics use severity glyphs (`✖ ▲ •`). Emoji are never
//!    emitted, because their width and rendering vary wildly across terminals.
//! 2. **Color is auto-gated, never unconditional.** ANSI escapes are stripped when
//!    stdout is not a terminal or when `NO_COLOR` is set, so piping into `jq`, `grep`,
//!    or a log aggregator never yields escape noise.
//! 3. **Glyphs degrade to ASCII on request.** `DOCGOV_ASCII=1` swaps the entire glyph
//!    alphabet for 7-bit ASCII, which keeps `TERM=dumb` and legacy Windows consoles
//!    legible without a separate code path.
//! 4. **Machine formats are pure data.** `json` and `github` renderers never emit
//!    decoration, timing, or advisory chatter; only `text` does.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::diagnostics::Severity;

/// Global presentation switches, resolved once at process start.
///
/// These are process-wide because the engine is a single-shot CLI: `render_*` helpers
/// are called from deep inside the rule/remote layers, and threading a context object
/// through every call site would add noise without buying anything.
static COLOR_ENABLED: AtomicBool = AtomicBool::new(false);
static ASCII_MODE: AtomicBool = AtomicBool::new(false);

/// Tri-state for `--color`, matching the `clap` `ValueEnum` surface.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

/// Resolve the global presentation switches from the CLI flag and the environment.
///
/// Precedence, highest first:
/// 1. Explicit `--color=always` / `--color=never`.
/// 2. The `NO_COLOR` convention (any non-empty value disables color).
/// 3. Terminal detection: color only when stdout is a TTY.
pub fn init(color: ColorChoice) {
    let enabled = match color {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => {
            std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
        }
    };
    COLOR_ENABLED.store(enabled, Ordering::Relaxed);

    let ascii = std::env::var_os("DOCGOV_ASCII")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);
    ASCII_MODE.store(ascii, Ordering::Relaxed);
}

pub fn color_enabled() -> bool {
    COLOR_ENABLED.load(Ordering::Relaxed)
}

pub fn ascii_mode() -> bool {
    ASCII_MODE.load(Ordering::Relaxed)
}

/// Symbol vocabulary for progress and result lines.
///
/// Each variant maps to exactly one glyph (or ASCII fallback) and one semantic role.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Sym {
    /// A check passed, a sync succeeded, an asset was already current.
    Success,
    /// A file or artifact was created.
    Create,
    /// A file or artifact was modified in place.
    Update,
    /// A file or artifact was removed / pruned.
    Remove,
    /// An advisory condition an operator may want to act on.
    Warning,
    /// A hard failure.
    Error,
    /// Neutral informational context (resolution, migration, telemetry).
    Info,
}

impl Sym {
    /// ASCII fallback glyph (`DOCGOV_ASCII=1`).
    fn ascii(self) -> &'static str {
        match self {
            Sym::Success => "[OK]",
            Sym::Create => "[+]",
            Sym::Update => "[~]",
            Sym::Remove => "[-]",
            Sym::Warning => "[!]",
            Sym::Error => "[x]",
            Sym::Info => "[i]",
        }
    }

    /// Unicode glyph, chosen for single-column width in modern terminals.
    fn unicode(self) -> &'static str {
        match self {
            Sym::Success => "✔",
            Sym::Create => "+",
            Sym::Update => "~",
            Sym::Remove => "-",
            Sym::Warning => "!",
            Sym::Error => "×",
            Sym::Info => "i",
        }
    }

    /// ANSI color code for this role, or `None` when color is disabled.
    fn color_code(self) -> Option<&'static str> {
        if !color_enabled() {
            return None;
        }
        Some(match self {
            Sym::Success => "32",              // green
            Sym::Create | Sym::Update => "33", // yellow
            Sym::Remove => "33",               // yellow
            Sym::Warning => "33",              // yellow
            Sym::Error => "31",                // red
            Sym::Info => "36",                 // cyan
        })
    }

    /// The glyph itself, already colored when color is active.
    pub fn glyph(self) -> String {
        let raw = if ascii_mode() {
            self.ascii()
        } else {
            self.unicode()
        };
        paint(raw, self.color_code())
    }
}

/// A single progress/result line: `{glyph} {message}`.
///
/// This is the *only* sanctioned way to print an operation status. Hard-coded
/// `println!("✔ ...")` calls are exactly what this module exists to eliminate.
pub fn status(sym: Sym, message: impl AsRef<str>) -> String {
    format!("{} {}", sym.glyph(), message.as_ref())
}

/// Wrap text in an ANSI color code unless color is disabled.
pub fn paint(text: &str, color: Option<&str>) -> String {
    match (color_enabled(), color) {
        (true, Some(code)) => format!("\x1b[{}m{}\x1b[0m", code, text),
        _ => text.to_string(),
    }
}

/// Semantic paint helpers, all gated by [`color_enabled`].
pub fn green(text: &str) -> String {
    paint(text, Some("32"))
}

pub fn yellow(text: &str) -> String {
    paint(text, Some("33"))
}

pub fn red(text: &str) -> String {
    paint(text, Some("31"))
}

pub fn cyan(text: &str) -> String {
    paint(text, Some("36"))
}

pub fn dimmed(text: &str) -> String {
    paint(text, Some("2"))
}

pub fn bold(text: &str) -> String {
    paint(text, Some("1"))
}

/// The color used for a diagnostic severity tag, or `None` when color is off.
pub fn severity_color(severity: &Severity) -> Option<&'static str> {
    Some(match severity {
        Severity::Error => "31",
        Severity::Warning => "33",
        Severity::Note => "36",
    })
}

/// The severity glyph prefixed to every rendered text diagnostic.
///
/// Deliberately *distinct* per severity: sharing one glyph between errors and
/// warnings (as the engine previously did) makes a scrolling log unreadable.
pub fn severity_glyph(severity: &Severity) -> String {
    let (unicode, ascii) = match severity {
        Severity::Error => ("×", "[x]"),
        Severity::Warning => ("!", "[!]"),
        Severity::Note => ("i", "[i]"),
    };
    let raw = if ascii_mode() { ascii } else { unicode };
    paint(raw, severity_color(severity))
}

/// `"1 error"` / `"2 errors"` — count-agreement helper so summaries never print
/// the awkward `error(s)` construction.
pub fn plural(count: usize, singular: &str, plural: &str) -> String {
    format!("{} {}", count, if count == 1 { singular } else { plural })
}

/// Bullet character for lists (`•` in Unicode, `*` in ASCII mode).
pub fn bullet() -> &'static str {
    if ascii_mode() {
        "*"
    } else {
        "•"
    }
}

/// Tree branch prefix (`├─ ` / `+- ` or `└─ ` / `\- `).
pub fn tree_branch(is_last: bool) -> &'static str {
    if ascii_mode() {
        if is_last {
            "\\- "
        } else {
            "+- "
        }
    } else if is_last {
        "└─ "
    } else {
        "├─ "
    }
}

/// Vertical pipe character (`│` in Unicode, `|` in ASCII mode).
pub fn pipe() -> &'static str {
    if ascii_mode() {
        "|"
    } else {
        "│"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes the process-global switch mutations so the tests below cannot
    /// interleave and observe each other's configuration.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_mode(color: bool, ascii: bool) {
        COLOR_ENABLED.store(color, Ordering::Relaxed);
        ASCII_MODE.store(ascii, Ordering::Relaxed);
    }

    #[test]
    fn unicode_glyphs_are_stable_when_color_is_off() {
        let _guard = LOCK.lock().unwrap();
        with_mode(false, false);

        assert_eq!(Sym::Success.glyph(), "✔");
        assert_eq!(Sym::Error.glyph(), "×");
        assert_eq!(Sym::Info.glyph(), "i");
        assert_eq!(status(Sym::Create, "Created ./a"), "+ Created ./a");

        // No escapes must leak into non-colored output.
        assert!(!status(Sym::Warning, "x").contains('\x1b'));
    }

    #[test]
    fn ascii_mode_swaps_the_entire_glyph_alphabet() {
        let _guard = LOCK.lock().unwrap();
        with_mode(false, true);

        assert_eq!(Sym::Success.glyph(), "[OK]");
        assert_eq!(Sym::Error.glyph(), "[x]");
        assert_eq!(severity_glyph(&Severity::Warning), "[!]");

        // The core promise of ASCII mode: no bytes outside 7-bit range.
        for sym in [
            Sym::Success,
            Sym::Create,
            Sym::Update,
            Sym::Remove,
            Sym::Warning,
            Sym::Error,
            Sym::Info,
        ] {
            assert!(sym.glyph().is_ascii(), "non-ASCII glyph for {sym:?}");
        }
        assert!(bullet().is_ascii());
        assert!(tree_branch(false).is_ascii());
        assert!(tree_branch(true).is_ascii());
        assert!(pipe().is_ascii());
    }

    #[test]
    fn color_is_emitted_only_when_enabled() {
        let _guard = LOCK.lock().unwrap();

        with_mode(true, false);
        let colored = Sym::Error.glyph();
        assert!(colored.starts_with("\x1b[31m"), "got {colored:?}");
        assert!(colored.ends_with("\x1b[0m"));

        with_mode(false, false);
        assert_eq!(Sym::Error.glyph(), "×");
    }

    #[test]
    fn severity_glyphs_are_distinct() {
        let _guard = LOCK.lock().unwrap();
        with_mode(false, false);

        let error = severity_glyph(&Severity::Error);
        let warning = severity_glyph(&Severity::Warning);
        let note = severity_glyph(&Severity::Note);
        assert_ne!(error, warning, "errors and warnings must be separable");
        assert_ne!(warning, note);
    }

    #[test]
    fn plural_agrees_with_count() {
        let _guard = LOCK.lock().unwrap();
        with_mode(false, false);

        assert_eq!(plural(0, "error", "errors"), "0 errors");
        assert_eq!(plural(1, "error", "errors"), "1 error");
        assert_eq!(plural(2, "error", "errors"), "2 errors");
    }
}
