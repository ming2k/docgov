---
id: ADR-0001
title: "Unified CLI Output Presentation Contract"
status: accepted
date: 2026-02-14
scope: cli/presentation
superseded_by: null
negative_knowledge: true
---

# 0001. Unified CLI Output Presentation Contract

- Status: Accepted
- Date: 2026-02-14
- Deciders: docgov maintainers
- Consulted: CI integrators, documentation tooling consumers

---

## Context and Problem Statement

Before this decision, every command hand-rolled its own terminal output at the point of
use. Across `cli.rs`, `remote.rs`, and `diagnostics.rs` there were 27 bare `println!`
sites, each choosing its own symbol, spacing, and color call ad hoc.

The result was an inconsistent surface that was hard to read and hard to automate
against:

- **Three symbol alphabets coexisted** — geometric (`✔ ✖ × ~ +`), emoji (`🔍 🚀 ✨ 💡`),
  and box-drawing (`╭─╰│·`). The same semantic role had no single representation.
- **Warnings and errors shared the `×` glyph.** Only the `[RULE-ID]` tag color differed,
  so a scrolled log could not be triaged by shape.
- **`--format` was silently ignored by `init` and `update`**, despite being a global flag.
- **JSON was a bare array.** A consumer could not distinguish "no findings" from "engine
  crashed", and had no schema handle for evolution.
- **Summary lines hard-coded `error(s)`** rather than agreeing with the count.
- **Fatal errors bypassed the format contract**, printing `Error: {:#}` prose to stderr
  even under `--format=json`.
- **Unicode decoration was unconditional**, so `TERM=dumb`, legacy Windows consoles, and
  ASCII-only log pipelines received mojibake.

This ADR records the contract that replaced that, so the invariants survive future
contributions made by humans and AI assistants alike.

## Decision Drivers

- **Machine-parseability**: CI must consume `json`/`github` output without scraping prose.
- **Scannability**: a human must triage a long report by glyph shape alone.
- **Portability**: no assumption of a Unicode-capable, color-capable terminal.
- **Single source of truth**: one module owns presentation, so the rules cannot drift apart.

## Considered Options

- Option 1: A single centralized `output` module owning all glyphs, color, and format gating.
- Option 2: Keep inline `println!` calls, but introduce shared glyph constants.
- Option 3: Adopt a third-party presentation crate (e.g. `console`, `indicatif`, `tabled`).

## Decision Outcome

Chosen option: "Option 1: A single centralized `output` module", because the presentation
rules are a *cross-cutting contract* between six call sites that must not be able to
diverge. Centralization makes the contract directly testable and makes the glyph alphabet
a single 7-variant enum instead of a scattered set of string literals.

`cli/src/output.rs` now owns:

- `Sym` — the 7-variant progress glyph alphabet (`Success Create Update Remove Warning
  Error Info`), each mapping to exactly one glyph.
- `ColorChoice` — the `--color=auto|always|never` tri-state.
- `severity_glyph` / `severity_color` — the distinct per-severity diagnostic glyphs.
- `plural` — count agreement, eliminating `error(s)`.
- `paint` and semantic helpers — the only sanctioned way to emit an escape sequence.

### Invariants & Behavioral Boundaries

- **[OUT-01] Single presentation owner.** Terminal symbols, ANSI color, and severity
  formatting must be produced via `crate::output`. New bare `println!("✔ …")` calls at
  command sites are prohibited; new code must call `output::status(Sym, …)`.
- **[OUT-02] Emoji are prohibited.** Only the glyphs in [`Sym`] and the severity set
  (`× ! i`) may be emitted. Emoji width and coverage vary across terminals, breaking both
  alignment and log pipelines.
- **[OUT-03] Color is gated, never unconditional.** ANSI escapes must be emitted only
  when color is enabled, resolved as: explicit `--color` > the `NO_COLOR` convention >
  `stdout.is_terminal()`. Piped output must contain zero escape bytes.
- **[OUT-04] Machine formats carry no decoration.** `--format=json` and `--format=github`
  emit pure data: no timing, no summaries, no advisory hints, no color.
- **[OUT-05] JSON is a versioned envelope.** `--format=json` emits
  `{schema_version, protocol_version, summary, diagnostics}`. It is never a bare array,
  including on the fatal-error path, which emits `{schema_version, error}`.
- **[OUT-06] ASCII degradation is an environment switch.** Setting `DOCGOV_ASCII=1`
  replaces the entire glyph alphabet with 7-bit ASCII; no code path may emit a non-ASCII
  byte when it is set.
- **[OUT-07] Severity glyphs are distinct.** `error`, `warning`, and `note` must never
  share a glyph; a report must be triageable without color.
- **[OUT-08] Exit codes are stable.** `0` success, `1` findings/violations, `2` engine
  failure.

### Positive Consequences

- A CI consumer parses one stable JSON shape and branches on `summary.errors`.
- `--color=never` yields a byte-reproducible capture, making golden-file tests possible.
- Adding a new status line is now a one-line call with a compile-time-checked glyph.
- The `colored` dependency was removed, since gating logic is now local and explicit.

### Negative Consequences & Trade-offs

- **Global mutable presentation state.** `output` uses process-wide atomics rather than
  threading a context object through the rule and remote layers. This is sound for a
  single-shot CLI but would be a data race in a long-lived multi-tenant host.
  - *Mitigation*: if the engine is ever embedded as a library server, the switches must
    move into an explicit `RenderContext` passed down the call chain.
- **Breaking change to the JSON contract.** Consumers of the pre-existing bare array must
  migrate to `report.diagnostics`. Accepted because the engine is pre-1.0.
  - *Mitigation*: `schema_version` makes the break detectable rather than silent.

## Rejected Alternatives & Negative Knowledge

### Option 2 (Rejected): Shared glyph constants with inline `println!`
- Why considered: Minimal diff; no new module; each command site stays self-contained.
- Why rejected: It standardizes the *symbols* while leaving the *structure* — indentation,
  spacing, format gating, the JSON envelope — duplicated at every call site. The original
  defects (un-gated color, format-blind commands, bare-array JSON) were structural, not
  lexical, so constants alone would have fixed the cosmetic half and left the real bugs.
  It also cannot express the invariant "every status line goes through one function".

### Option 3 (Rejected): Third-party presentation crates (`console`, `indicatif`, `tabled`)
- Why considered: Battle-tested Unicode width handling, progress bars, and table rendering
  would come for free, and `console` would centralize `is_terminal` detection.
- Why rejected: It conflicts with the engine's stated zero-vendoring posture and inflates
  the dependency tree of a tool whose entire value proposition is a single self-contained
  binary. `indicatif`'s animated progress bars are also actively harmful here: the output
  must be byte-reproducible and diffable in CI, and animations are non-deterministic.
  The drawing primitives actually needed are a 7-glyph enum and one SGR wrapper.

### Emoji-preserving output (Rejected)
- Why considered: Emoji are visually distinctive and the engine already shipped them.
- Why rejected: Emoji render at inconsistent widths (frequently double-width), which
  misaligns any columnar output, and their coverage varies by terminal and font. Several
  (`🔍`, `🚀`) signalled *process* state ("resolving…", "upgrading…"), which duplicates
  information already present in the sentence and adds noise to non-interactive logs.

### Keeping all `println!` sites but gating color per-call (Rejected)
- Why considered: Addresses the mojibake and color-leak issues with the smallest change.
- Why rejected: Leaves `--format` unenforced at `init`/`update`, leaves the JSON contract
  as a bare array, and leaves severity glyphs indistinguishable. It treats symptoms.

## Links

- Implementation: [`cli/src/output.rs`](../../cli/src/output.rs), [`cli/src/diagnostics.rs`](../../cli/src/diagnostics.rs), [`cli/src/cli.rs`](../../cli/src/cli.rs)
- Tests: [`cli/tests/cli_tests.rs`](../../cli/tests/cli_tests.rs) (the "CLI Output Contract Tests" section)
- Registry: [index.md](index.md)
