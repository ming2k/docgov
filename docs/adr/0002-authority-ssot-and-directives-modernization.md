---
id: ADR-0002
title: "Sole Declarative Authority in .docgov.yml and AI Directives Modernization"
status: accepted
date: 2026-03-31
scope: cli/governance/spec
superseded_by: null
negative_knowledge: true
---

# 0002. Sole Declarative Authority in .docgov.yml and AI Directives Modernization

- Status: Accepted
- Date: 2026-03-31
- Deciders: docgov maintainers
- Consulted: AI tool integrators, repository maintainers, RFC-0002 reviewers

---

## Context and Problem Statement

Two architectural tensions emerged in downstream usage of `docgov`:

1. **Inversion of Authority between Manifest and Lockfile**:
   `update_repo` prioritized `.docgov.lock` over `.docgov.yml` when determining the current specification version and upstream reference (`lock.as_ref().map(...).unwrap_or(cfg...)`). When a human maintainer updated `upstream.ref` in `.docgov.yml`, the engine continued reading the stale lockfile, ignoring the maintainer's explicit intent. This violated standard declarative conventions (e.g. `Cargo.toml` vs `Cargo.lock`).
2. **Cognitive Metaphor Inflation in AI Directives**:
   The canonical `directives.snippet` and `AGENTS.md` directives block relied on pseudo-academic and implementation-centric framing ("Machine Invariants vs Cognitive Protocols", "4D Coordinate Tensor", "Constitution", "Contributor Firewall"). Furthermore, `directives.snippet` omitted `[INV-AGENT-04] Architectural Significance Threshold`, while strict string matching failed when linters or LLMs reformatted delimiter whitespaces. Finally, lint diagnostics suggested running `docgov init`, inducing psychological friction around potential destructive scaffolding.

This ADR codifies the long-term, modern architectural standard resolving these issues.

## Decision Drivers

- **Declarative Single Source of Truth (SSOT)**: Human-authored configuration (`.docgov.yml`) must command absolute authority over desired state; lockfiles must never override manifest intent.
- **Cognitive Clarity for LLMs & Humans**: Directives must follow an unambiguous 4-step execution funnel using standard industrial terminology.
- **Robust Boundary Defense**: Delimiter detection must tolerate arbitrary inner whitespace and provide resilient anchor fallback.
- **Idempotent Self-Correction**: The engine must provide a dedicated, non-destructive `docgov fix` command to repair repairable directives violations.

## Considered Options

- **Option 1 (Chosen)**: Establish `.docgov.yml` as the sole declarative SSOT; relegate `.docgov.lock` strictly to integrity verification; modernize `directives.snippet` to an industrial 4-step execution funnel; introduce whitespace-tolerant delimiter matching and a dedicated `docgov fix` command.
- **Option 2**: Eliminate `.docgov.lock` entirely and embed cryptographic hashes as metadata comments in `.docgov.yml`.
- **Option 3**: Retain lockfile priority and require maintainers to run `docgov update --target` rather than editing `.docgov.yml`.

## Decision Outcome

Chosen option: **Option 1**, because it honors the proven separation of concerns between declarative intent (human manifest) and cryptographic integrity (machine lockfile), while eliminating cognitive friction for coding agents.

### Invariants & Architectural Contracts

- **`[AUTH-01]` Sole Declarative Authority**: `.docgov.yml` is the sole source of truth for repository governance configuration, protocol version, and upstream specification reference. The engine must evaluate current state from `.docgov.yml`.
- **`[AUTH-02]` Lockfile Subordination**: `.docgov.lock` is an integrity artifact storing cryptographic sha256 digests and resolution timestamps. It must never override `.docgov.yml`. If `.docgov.yml` and `.docgov.lock` declare different versions or refs, the lockfile is considered stale and must be regenerated during sync.
- **`[AUTH-03]` 4-Step Directives Execution Funnel**: AI Directives are organized into four sequential sections:
  1. `Hard Constraints (Zero-Tolerance Invariants)`: Deterministic repository boundaries (`INV-LINT-01` through `05`).
  2. `Architectural Decision Standards (Reasoning Principles)`: Cognitive reasoning invariants (`INV-AGENT-01` through `04`).
  3. `Reference Specifications (Authoritative Standards)`: Authoritative governance links.
  4. `Verification & Self-Correction Gate`: Execution mandate (`docgov check`) with self-healing instruction.
- **`[AUTH-04]` Delimiter Whitespace Tolerance**: Boundary comments `<!-- BEGIN DOCGOV DIRECTIVES -->` and `<!-- END DOCGOV DIRECTIVES -->` must be parsed with case-insensitive whitespace tolerance, accepting arbitrary spaces around delimiters without failing validation.
- **`[AUTH-05]` First-Class `docgov fix`**: Repairable invariant violations (including stripped or corrupted agent directives boundary comments) must be fixable via `docgov fix` and `docgov check --fix`. Diagnostics must recommend `docgov fix` rather than `docgov init`.

### Positive Consequences

- Maintainers can bump versions directly in `.docgov.yml`; CLI commands and CI will faithfully honor the change.
- LLMs operating in the repository encounter standard software engineering vocabulary, reducing prompt decoding overhead.
- AI formatters (Prettier, markdownlint) modifying delimiter whitespace will no longer trigger false-positive CI failures.
- Non-destructive `docgov fix` provides an unambiguous, safe self-healing path for both agents and human developers.

### Negative Consequences & Trade-offs

- Downstream repositories running older CLI versions must upgrade the CLI to receive `docgov fix` and relaxed delimiter parsing.
- Updating `directives.snippet` alters the canonical SHA256 digest, requiring downstream projects to refresh lockfiles on next protocol update.

## Rejected Alternatives & Negative Knowledge

### Option 2 (Rejected): Eliminating `.docgov.lock` entirely
- **Why considered**: Removes an extra file; mimics single-file tools like `pre-commit`.
- **Why rejected**: In `docgov`, `AGENTS.md` is a living file mutated in the repository root. Without a distinct `.docgov.lock`, verifying whether an LLM tampered with the directives block would require writing machine-generated hashes into `.docgov.yml`, polluting human comments and generating git merge conflicts. Furthermore, offline verification in `< 50ms` requires pinned cryptographic digests.

### Option 3 (Rejected): Retaining Lockfile Priority
- **Why considered**: Preserves existing code paths without changes.
- **Why rejected**: Creates cognitive dissonance where editing a configuration file has no effect until a secondary command is invoked. Contradicts standard developer expectations established by Cargo, Bundler, and Terraform.
