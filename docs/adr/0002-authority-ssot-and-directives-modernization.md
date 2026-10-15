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
2. **Cognitive Metaphor Inflation & Context Pollution in AI Directives**:
   The legacy directives block in `AGENTS.md` relied on pseudo-academic framing ("Machine Invariants vs Cognitive Protocols", "4D Coordinate Tensor", "Constitution", "Contributor Firewall") and duplicated deterministic `INV-LINT-*` linter checks into always-on LLM prompt context. Furthermore, it used an obscure `.snippet` filename outside `.manifest.json`, omitted `[INV-AGENT-04] Architectural Significance Threshold`, and failed when linters or LLMs reformatted delimiter whitespaces. Finally, lint diagnostics suggested running `docgov init`, inducing psychological friction around potential destructive scaffolding.

This ADR codifies the long-term, modern architectural standard resolving these issues.

## Decision Drivers

- **Declarative Single Source of Truth (SSOT)**: Human-authored configuration (`.docgov.yml`) must command absolute authority over desired state; lockfiles must never override manifest intent.
- **Zero-Pollution Progressive Disclosure (`default-agents.md`)**: `AGENTS.md` must remain minimal, scoped strictly to documentation/ADR changes, and delegate deterministic `INV-LINT-*` enforcement to `docgov check` and detailed specs to `docs/governance/documentation/`.
- **Robust Boundary Defense**: Delimiter detection must tolerate arbitrary inner whitespace and provide resilient anchor fallback, with explicit comment warnings against manual edits inside the hash-verified block.
- **Idempotent Self-Correction**: The engine must provide a dedicated, non-destructive `docgov fix` command to repair repairable directives violations.

## Considered Options

- **Option 1 (Chosen)**: Establish `.docgov.yml` as the sole declarative SSOT; relegate `.docgov.lock` strictly to integrity verification; standardize the agent template as `spec/default-agents.md` (symmetric with `spec/default-config.yml` and registered in `spec/.manifest.json`) using a minimal scope-gated routing block; introduce whitespace-tolerant delimiter matching and a dedicated `docgov fix` command.
- **Option 2**: Eliminate `.docgov.lock` entirely and embed cryptographic hashes as metadata comments in `.docgov.yml`.
- **Option 3**: Retain lockfile priority and require maintainers to run `docgov update --target` rather than editing `.docgov.yml`.

## Decision Outcome

Chosen option: **Option 1**, because it honors the proven separation of concerns between declarative intent (human manifest), cryptographic integrity (machine lockfile), deterministic AST linting (`INV-LINT-*` via `docgov check`), and minimal progressive-disclosure agent routing (`default-agents.md`).

### Invariants & Architectural Contracts

- **`[AUTH-01]` Sole Declarative Authority**: `.docgov.yml` is the sole source of truth for repository governance configuration, protocol version, and upstream specification reference. The engine must evaluate current state from `.docgov.yml`.
- **`[AUTH-02]` Lockfile Subordination**: `.docgov.lock` is an integrity artifact storing cryptographic sha256 digests and resolution timestamps. It must never override `.docgov.yml`. If `.docgov.yml` and `.docgov.lock` declare different versions or refs, the lockfile is considered stale and must be regenerated during sync.
- **`[AUTH-03]` Minimal Scope-Gated Agent Block (`default-agents.md`)**: Registered in `spec/.manifest.json` alongside `default-config.yml`, `default-agents.md` embeds a compact, scope-gated `## Documentation Governance` block that:
  1. Explicitly warns humans and AI in an HTML comment not to edit the block or its delimiters (hash-verified via `.docgov.lock`).
  2. Scopes activation strictly to documentation, ADR, or code-doc synced changes (no-op during pure code changes or Q&A).
  3. Routes agents to on-demand specs (`docs/governance/documentation/`), core ADR reasoning principles (`INV-AGENT-01/02/04`), and lazy verification (`docgov check`).
- **`[AUTH-04]` Delimiter Whitespace Tolerance**: Boundary comments `<!-- BEGIN DOCGOV DIRECTIVES -->` and `<!-- END DOCGOV DIRECTIVES -->` must be parsed with case-insensitive whitespace tolerance, accepting arbitrary spaces around delimiters without failing validation.
- **`[AUTH-05]` First-Class `docgov fix`**: Repairable invariant violations (including stripped or corrupted agent directives boundary comments and drifted governance spec mirrors) must be fixable via `docgov fix` and `docgov check --fix`. Diagnostics must recommend `docgov fix` rather than `docgov init`.
- **`[AUTH-06]` Bifurcated Enforcement Model**:
  1. *Strict Managed-Zone Protection (`[INV-LINT-05]`, Blocking `Error`)*: Surfaces maintained exclusively by `docgov` (`AGENTS.md` directives block and `docs/governance/documentation/**` spec mirror) must never be manually modified by developers or AI. Hash drift triggers fatal errors (`Error`) and blocks CI until repaired via `docgov fix`.
  2. *Permissive User-Space Advisories (`[INV-LINT-01/03/04]`, Non-Blocking `Warning`)*: User-authored files receive constructive guidance (`Warning`, exit code 0) without obstructing pull requests or developer velocity.
- **`[AUTH-07]` Invariant Deprecation SOP**: Retired invariants are permanently tombstoned without renumbering IDs; parsers accept legacy keys as no-ops; deprecations are codified with Negative Knowledge.

### Positive Consequences

- Maintainers can bump versions directly in `.docgov.yml`; CLI commands and CI will faithfully honor the change.
- Host `AGENTS.md` files remain pure and unpolluted during everyday coding tasks, eliminating token waste and prompt hijacking.
- AI formatters (Prettier, markdownlint) modifying delimiter whitespace will no longer trigger false-positive CI failures.
- Non-destructive `docgov fix` provides an unambiguous, safe self-healing path for both agents and human developers, restoring both directives and governance mirrors.
- CI is protected from dogmatic user-space blockages while strictly guaranteeing the integrity of `docgov`-managed governance assets.

### Negative Consequences & Trade-offs

- Downstream repositories running older CLI versions must upgrade the CLI to receive `docgov fix` and relaxed delimiter parsing.
- Updating `default-agents.md` alters the canonical SHA256 digest, requiring downstream projects to refresh lockfiles on next protocol update.

## Rejected Alternatives & Negative Knowledge

### Rejection of Dogmatic User-Space Enforcement (`[INV-LINT-02]` Contributor Firewall)
- **Why considered**: `INV-LINT-02` mechanically prevented public user-facing documentation from containing relative links into internal developer spaces (`docs/dev/**`).
- **Why rejected & deprecated**: Over-prescriptive in user space; treated benign cross-references as fatal errors, creating developer friction and violating the core philosophy of permissive user-space guidance. Retired under the Invariant Deprecation SOP.

### Option 2 (Rejected): Eliminating `.docgov.lock` entirely
- **Why considered**: Removes an extra file; mimics single-file tools like `pre-commit`.
- **Why rejected**: In `docgov`, `AGENTS.md` is a living file mutated in the repository root. Without a distinct `.docgov.lock`, verifying whether an LLM tampered with the directives block would require writing machine-generated hashes into `.docgov.yml`, polluting human comments and generating git merge conflicts. Furthermore, offline verification in `< 50ms` requires pinned cryptographic digests.

### Option 3 (Rejected): Retaining Lockfile Priority
- **Why considered**: Preserves existing code paths without changes.
- **Why rejected**: Creates cognitive dissonance where editing a configuration file has no effect until a secondary command is invoked. Contradicts standard developer expectations established by Cargo, Bundler, and Terraform.
