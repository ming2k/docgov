# System Invariants

This document codifies the architectural, documentation, and governance invariants across repositories adopting `docgov`.

Governance invariants are partitioned into three operational tiers:
- **Managed-Zone Integrity (`INV-LINT-05`)**: Strict, machine-enforced boundary defense protecting surfaces maintained exclusively by `docgov`. Violations block verification (`Error`) and are self-healed via `docgov fix`.
- **Cognitive & Agent Protocol Layer (`INV-AGENT-*`)**: Non-mechanical architectural reasoning standards guiding AI assistant judgment, context routing, and negative knowledge.
- **User-Space Advisory Standards (`INV-LINT-01/03/04`)**: Permissive, constructive guidance on user-authored files (`Warning`), assisting developers without blocking pull requests.

---

## 1. Managed-Zone Integrity (`INV-LINT-05`)

Strictly enforced by `docgov check`. Developers and AI coding assistants must never manually edit, reorganize, or tamper with `docgov`-maintained surfaces. Violations produce fatal errors (`Error`) and must be restored using `docgov fix` or `docgov update`.

- **`[INV-LINT-05]` Managed Governance Assets Integrity**:
  1. **Agent Directives Target**: AI instruction files (e.g. `AGENTS.md`) must contain the canonical `default-agents.md` block enclosed within exact boundary comments (`<!-- BEGIN DOCGOV DIRECTIVES -->` and `<!-- END DOCGOV DIRECTIVES -->`). Content hash must match `.docgov.lock`.
  2. **Specification Mirror**: The governance documentation mirror (`docs/governance/documentation/`) is a read-only projection of upstream standards. Manual edits or missing files fail hash verification against `.docgov.lock`.

---

## 2. Cognitive & Agent Protocol Layer (`INV-AGENT`)

Enforced by progressive-disclosure AI assistant directives (`AGENTS.md`), domain specification profiles, and human architectural peer reviews.

- **`[INV-AGENT-01]` Negative Knowledge Mandate**:
  Every Architectural Decision Record (ADR) must contain a dedicated "Rejected Alternatives & Negative Knowledge" section detailing which alternatives were considered and why they failed. Decisions without explicit failure modes must be rejected.
- **`[INV-AGENT-02]` Context Routing & Historical State**:
  - *Generative Isolation*: When generating code or proposing new implementations, AI agents must filter out documents with `status: superseded` or `status: rejected` to prevent resurrecting dead patterns.
  - *Forensic Retrieval*: When prompted for refactoring, historical rationale, or root-cause investigation, AI agents must actively retrieve superseded/rejected records and cite their documented failure modes as negative constraints.
- **`[INV-AGENT-03]` Blameless Postmortem Structure (Operations Profile)**:
  Post-incident reviews in `docs/dev/postmortems/` must strictly focus on timeline reconstruction, detection gaps, and systemic defense-in-depth failures. Attribution of personal human error or developer blame is strictly prohibited.
- **`[INV-AGENT-04]` Architectural Significance Threshold**:
  An ADR must only be authored for decisions satisfying at least two criteria of the 3-Question Significance Test (high reversal cost, cross-boundary blast radius, or generating binding invariants). Trivial implementation details, localized function refactorings, or ephemeral workarounds must never be admitted to `docs/adr/`.

---

## 3. User-Space Advisory Standards (`INV-LINT`)

Constructive guidance on user-authored documentation. Violations emit advisory warnings (`Warning`), highlighting potential hygiene issues without blocking continuous integration or developer workflows.

- **`[INV-LINT-01]` Location Sanitization (Root Whitelist)**:
  Encourages routing arbitrary documentation into `docs/`. Whitelisted root Markdown files (`README.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, `AGENTS.md`, `LICENSE.md`, `SECURITY.md`) are standard.
- **`[INV-LINT-03]` Frontmatter Schema & Lifecycle Integrity**:
  Encourages valid YAML frontmatter (`id`, `title`, `status`, `date`) on records in `docs/adr/`. Promotes in-place evolution (`status: superseded` with `superseded_by: ADR-NNNN`) over physical archive moves.
- **`[INV-LINT-04]` Code-Doc Synchronization Trigger**:
  Advises synchronizing corresponding documentation surfaces when monitored source code is modified, evaluated via Git diff analysis.

---

## 4. Invariant Lifecycle & Deprecation SOP

To maintain immutable protocol history, backwards compatibility, and zero-legacy clarity across monorepos and downstream adopting projects, all invariant retirements must adhere to this Standard Operating Procedure:

### Rule 1: ID Immutability & Tombstoning (Never Renumber)
Once assigned, an invariant identifier (such as `[INV-LINT-02]`) is permanent. Retired invariants are tombstoned in the Deprecated Invariants Registry below. Remaining rules are **never renumbered**, preventing downstream configuration drift or broken historical references.

### Rule 2: Configuration No-Op Tolerance
When an invariant is retired, CLI parsers must treat its configuration key in `.docgov.yml` as a harmless no-op rather than failing deserialization. Downstream repositories do not need immediate config migration upon upgrading `docgov`.

### Rule 3: Three-Step Deprecation Procedure
1. **Spec Tombstone**: Mark the invariant as deprecated in `spec/core/invariants.md`, record its retirement version and technical rationale in the registry table, and update `spec/.manifest.json`.
2. **Engine Deactivation**: Remove active evaluation passes from `cli/src/rules/` while maintaining deserialization tolerance.
3. **ADR Ratification**: Codify the retirement rationale in `docs/adr/` as binding Negative Knowledge (`[INV-AGENT-01/02]`), ensuring future contributors and AI agents do not resurrect the discarded pattern.

### Deprecated Invariants Registry

| Invariant ID | Canonical Name | Status | Deprecated In | Rationale |
| :--- | :--- | :--- | :--- | :--- |
| **`[INV-LINT-02]`** | Contributor Firewall | Deprecated (Tombstoned) | v0.0.4 | Over-prescriptive enforcement on user-space internal relative links; contradicted permissive advisory philosophy and created artificial friction between public and developer docs. |
