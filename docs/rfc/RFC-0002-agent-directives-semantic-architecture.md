# RFC-0002: Re-architecting Agent Directives — Semantic Modernization, Robust Boundary Defense & Verification Closed-Loop

- Status: Consensus Reached
- Author: docgov core team
- Created: 2026-03-31
- Last Updated: 2026-03-31
- Target ADR: docs/adr/0002-authority-ssot-and-directives-modernization.md
- Discussion: https://github.com/ming2k/docgov/discussions

---

> **Notice**: This RFC is a pre-decision deliberation document. It explores possibilities
> and solicits stakeholder feedback. **It does not represent binding architectural policy.**
> For authoritative constraints, consult accepted records in `docs/adr/`.

---

## 1. Summary

This RFC proposes a comprehensive architectural modernization and robustness overhaul of `docgov`'s AI Agent Governance Directives (`default-agents.md` and `AGENTS.md`):
1. **Progressive Disclosure & Scope Gating**: Eliminate academic metaphors and redundant mechanical linter (`INV-LINT-*`) repetition from `AGENTS.md` in favor of a compact, scope-gated routing block (`## Documentation Governance`).
2. **Symmetric Asset Naming (`default-agents.md`)**: Rename the obscure `directives.snippet` to `spec/default-agents.md` (symmetric with `spec/default-config.yml`) and register it as a first-class template in `spec/.manifest.json`.
3. **Architectural Determinism (Rejection of Dynamic Templating)**: Reaffirm that `default-agents.md` remains a static, versioned, cryptographic-hashable Sovereign Spec Asset, rejecting dynamic template rendering as unnecessary over-engineering.
4. **Anti-Corruption & Boundary Robustness**: Support relaxed whitespace delimiter matching, explicit anti-tampering HTML comments, inline anchor fallbacks, and introduce a dedicated, idempotent `docgov fix` (or `docgov check --fix`) self-healing workflow.

---

## 2. Motivation & Problem Statement

### 2.1 Metaphor Inflation & Cognitive Friction
The existing directives snippet suffers from metaphor inflation:
- Classifying rules into "Machine Invariants" vs "Cognitive Protocols" reflects compiler implementation internals rather than the agent's task execution lifecycle.
- Terms such as "4D Coordinate Tensor" (borrowed from Diátaxis), "Constitution", and "Link Contracts" create unnecessary decoding overhead for both LLM attention windows and human engineers.
- Tautological phrasing (e.g. "Canonical Reference Standards") adds verbosity without increasing semantic precision.

### 2.2 Protocol Drift & Incompleteness
`spec/core/invariants.md` defines cognitive invariants (including `[INV-AGENT-04]` to prevent agents from creating trivial ADRs for localized refactorings), while mechanical rules (`INV-LINT-01..05`) are deterministically enforced by `docgov check`. Repeating mechanical linter rules in `AGENTS.md` wastes context tokens and hijacks agent attention during non-documentation tasks.

### 2.3 Boundary Fragility in Multi-Agent Environments
In contemporary multi-agent environments (Cursor, Claude Code, GitHub Copilot, Prettier, dprint):
- **Whitespace Sensitivity**: Naive exact string matching (`text.match_indices(DOCGOV_DIRECTIVES_BEGIN)`) fails when automated formatters insert spaces (e.g. `<!--  BEGIN ...  -->`).
- **Comment Stripping**: Several prompt extractors strip HTML comments to conserve tokens, causing agents to unknowingly drop boundary tags upon rewriting.
- **Repair Psychological Barrier**: Directing users to run `docgov init` on lint failure triggers fears of destructive overwrites; an explicit, idempotent `fix` command is needed.

---

## 3. Proposed Design

### 3.1 Minimal Scope-Gated `default-agents.md`
Replace `directives.snippet` with `spec/default-agents.md`:

```markdown
<!-- BEGIN DOCGOV DIRECTIVES -->
<!-- MANAGED BY DOCGOV: DO NOT EDIT THIS BLOCK OR ITS DELIMITERS. Manual changes will fail .docgov.lock hash verification and be overwritten by `docgov fix`. -->
## Documentation Governance <!-- @docgov-anchor -->

> **Scope**: Applies ONLY when creating or modifying documentation (`docs/**`, root `.md`), ADRs (`docs/adr/`), or code-doc synced paths. Skip during pure code changes or Q&A.

1. **Read Specs on Demand**: Before drafting or restructuring docs, inspect `docs/governance/documentation/` (`core/taxonomy.md`, `core/invariants.md`, `core/style.md`, and `profiles/architecture/adr.md`).
2. **ADR Reasoning (`[INV-AGENT-01/02/04]`)**: Author ADRs only for high-impact architectural decisions (skip trivial refactorings); always document rejected alternatives; never use `superseded`/`rejected` ADRs as active patterns.
3. **Lazy Verification**: Run `docgov check` before final hand-off ONLY when documentation, governance files, or monitored source paths (`[INV-LINT-04]`) are modified.
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
