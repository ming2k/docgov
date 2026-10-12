# RFC-0002: Re-architecting Agent Directives — Semantic Modernization, Robust Boundary Defense & Verification Closed-Loop

- Status: Under Review
- Author: docgov core team
- Created: 2026-03-31
- Last Updated: 2026-03-31
- Target ADR: docs/adr/0002-agent-directives-semantic-architecture.md
- Discussion: https://github.com/ming2k/docgov/discussions

---

> **Notice**: This RFC is a pre-decision deliberation document. It explores possibilities
> and solicits stakeholder feedback. **It does not represent binding architectural policy.**
> For authoritative constraints, consult accepted records in `docs/adr/`.

---

## 1. Summary

This RFC proposes a comprehensive architectural modernization and robustness overhaul of `docgov`'s AI Agent Governance Directives (`directives.snippet` and `AGENTS.md`):
1. **Semantic De-obfuscation & Funnel Reorganization**: Eliminate academic metaphors ("Cognitive Protocols", "4D Coordinate Tensor", "Constitution") in favor of an industrial-grade 4-step execution funnel (Hard Constraints $\rightarrow$ Architectural Decision Standards $\rightarrow$ Reference Specifications $\rightarrow$ Verification & Self-Correction Gate).
2. **Protocol Completeness**: Incorporate the omitted `[INV-AGENT-04] Architectural Significance Threshold` into the directives block, and introduce an actionable self-correction mandate upon verification failure.
3. **Architectural Determinism (Rejection of Dynamic Templating)**: Reaffirm that `directives.snippet` remains a static, versioned, cryptographic-hashable Sovereign Spec Asset, rejecting dynamic template rendering as unnecessary over-engineering.
4. **Anti-Corruption & Boundary Robustness**: Support relaxed whitespace delimiter matching, provide inline anchor fallbacks, and introduce a dedicated, idempotent `docgov fix` (or `docgov check --fix`) self-healing workflow.

---

## 2. Motivation & Problem Statement

### 2.1 Metaphor Inflation & Cognitive Friction
The existing directives snippet suffers from metaphor inflation:
- Classifying rules into "Machine Invariants" vs "Cognitive Protocols" reflects compiler implementation internals rather than the agent's task execution lifecycle.
- Terms such as "4D Coordinate Tensor" (borrowed from Diátaxis), "Constitution", and "Link Contracts" create unnecessary decoding overhead for both LLM attention windows and human engineers.
- Tautological phrasing (e.g. "Canonical Reference Standards") adds verbosity without increasing semantic precision.

### 2.2 Protocol Drift & Incompleteness
`spec/core/invariants.md` defines four cognitive invariants (including `[INV-AGENT-04]` to prevent agents from creating trivial ADRs for localized refactorings). However, `directives.snippet` currently omits `[INV-AGENT-04]`.

### 2.3 Boundary Fragility in Multi-Agent Environments
In contemporary multi-agent environments (Cursor, Claude Code, GitHub Copilot, Prettier, dprint):
- **Whitespace Sensitivity**: Naive exact string matching (`text.match_indices(DOCGOV_DIRECTIVES_BEGIN)`) fails when automated formatters insert spaces (e.g. `<!--  BEGIN ...  -->`).
- **Comment Stripping**: Several prompt extractors strip HTML comments to conserve tokens, causing agents to unknowingly drop boundary tags upon rewriting.
- **Repair Psychological Barrier**: Directing users to run `docgov init` on lint failure triggers fears of destructive overwrites; an explicit, idempotent `fix` command is needed.

---

## 3. Proposed Design

### 3.1 Directive Snippet Reorganization
Restructure `directives.snippet` into a streamlined 4-step execution funnel:

```markdown
<!-- BEGIN DOCGOV DIRECTIVES -->
<!-- CRITICAL: MANAGED BY DOCGOV. DO NOT MODIFY, MOVE, OR REMOVE DELIMITER TAGS. -->
## Documentation Governance Directives <!-- @docgov-anchor -->

All modifications must adhere to repository invariants and pass `docgov check`.

### 1. Hard Constraints (Zero-Tolerance Invariants)
- `[INV-LINT-01] Root Whitelist`: Never create arbitrary Markdown files at the repository root. Only whitelisted files (`README.md`, `CONTRIBUTING.md`, `AGENTS.md`, etc.) are allowed.
- `[INV-LINT-02] Scope Boundary`: Public documentation (`docs/{tutorials,how-to,reference,explanation}/`) MUST NEVER link to internal contributor docs (`docs/dev/**`).
- `[INV-LINT-03] Frontmatter Schema`: Every ADR in `docs/adr/` must declare valid YAML frontmatter (`id`, `title`, `status`, `date`).
- `[INV-LINT-04] Code-Doc Sync`: Modifying monitored paths in `src/` requires updating corresponding `docs/` in the same change.
- `[INV-LINT-05] Directives Integrity`: Never remove, reformat, or strip boundary delimiters (`<!-- BEGIN/END DOCGOV DIRECTIVES -->`) when modifying agent configuration files.

### 2. Architectural Decision Standards (Reasoning Principles)
- `[INV-AGENT-01] Rejected Alternatives`: Every new ADR MUST document discarded options and their specific failure modes.
- `[INV-AGENT-02] Historical State Routing`:
  - New implementations: DO NOT use `superseded` or `rejected` ADRs as active patterns.
  - Refactoring & investigations: MUST inspect `superseded` ADRs to learn from historical failure modes.
- `[INV-AGENT-03] Blameless Postmortems`: Incident postmortems must focus strictly on timeline, detection gaps, and systemic defense failures, prohibiting personal human blame.
- `[INV-AGENT-04] Architectural Significance Threshold`: Only author ADRs for decisions with high reversal cost or cross-boundary blast radius (skip localized or trivial refactorings).

### 3. Reference Specifications (Authoritative Standards)
Before drafting or restructuring documentation, inspect the local governance specifications:
- Taxonomy & Layout (Diátaxis): `docs/governance/documentation/core/taxonomy.md`
- System Invariants: `docs/governance/documentation/core/invariants.md`
- Style & Hyperlink Rules: `docs/governance/documentation/core/style.md`
- ADR & RFC Standards: `docs/governance/documentation/profiles/architecture/adr.md`
- Quality & Verification: `docs/governance/documentation/profiles/validation/testing.md`

### 4. Verification & Self-Correction Gate
Run verification before completing any task:
```bash
docgov check
```
If violations are detected, inspect diagnostic suggestions and resolve them within the current turn.
<!-- END DOCGOV DIRECTIVES -->
```

### 3.2 Reaffirmation of Static Sovereign Spec Distribution
Dynamic templating (e.g. Jinja/Tera rendering of paths) is rejected. The snippet remains an immutable, versioned, cryptographic-hashable artifact pinned in `.docgov.lock`.

### 3.3 Engine-Level Boundary Defense & Self-Healing
1. **Relaxed Regex Delimiter Matching**: Replace exact substring matching with regex patterns tolerant to inner whitespace: `<!--\s*BEGIN DOCGOV DIRECTIVES\s*-->` and `<!--\s*END DOCGOV DIRECTIVES\s*-->`.
2. **Inline Anchor Fallback**: Embed `<!-- @docgov-anchor -->` on the main heading as a secondary anchor if outer block comments are stripped.
3. **First-Class `docgov fix` Command**: Expose the non-invasive `patch_agent_directives` repair logic via `docgov fix` and `docgov check --fix`.

---

## 4. Drawbacks & Trade-offs

- **Lockfile Hash Drift**: Updating the canonical snippet requires running `docgov update` in downstream repositories to synchronize the lockfile. This is mitigated by bumping the protocol spec version cleanly.
- **Microsecond Parsing Overhead**: Compiling and evaluating regular expressions during linting introduces sub-millisecond overhead, easily staying within the `< 50ms` execution budget.

---

## 5. Alternatives Considered

- **Alternative A: Dynamic Jinja/Tera Templating**:
  - *Pros*: Could theoretically inject custom ADR paths from `.docgov.yml`.
  - *Cons*: Destroys deterministic lockfile hash verification, introduces runtime dependencies, and contradicts the role of invariants as universal rules.
- **Alternative B: Deprecating HTML Comments in Favor of Pure Headings**:
  - *Pros*: Cannot be stripped by comment-cleaning prompt loaders.
  - *Cons*: Prone to false positive boundary truncations in repositories with varied Markdown styling.

---

## 6. Unresolved Questions

- [ ] Should `docgov fix` and `docgov check --fix` be aliases of the same underlying workflow?
- [ ] Should `docs/governance/` reference links gracefully degrade to remote spec URLs when local mirrors are not adopted?

---

## 7. Resolution & Next Steps

1. Complete community and maintainer review on RFC-0002.
2. Distill agreed invariants into `docs/adr/0002-agent-directives-semantic-architecture.md`.
3. Implement snippet updates and engine robustness enhancements in CLI and Spec releases.
