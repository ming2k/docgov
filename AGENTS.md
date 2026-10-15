# AGENTS.md

Instructions for AI coding assistants working in the `docgov` repository.

---

## 1. Repository Identity & Mission

`docgov` (Document Governance) is a portable, protocol-versioned documentation governance standard and verification engine for software engineering repositories.

The repository is structured as a monorepo:
1. **`spec/`**: Sovereign specification standard (Core governance primitives, vertical domain profiles).
2. **`cli/`**: High-performance compiler-grade Rust verification engine (`docgov`).

---

## 2. Monorepo Guardrails

- **CLI Engine**: All engine source code resides in `cli/src/` with integration test suites in `cli/tests/`.
- **Specification**: All governance specifications reside in `spec/`.
- **Independent Versioning**: CLI uses `vx.x.x` tags; Spec uses `spec-vx.x.x` tags.
- **Verification**: Run `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` before completing code changes.

<!-- BEGIN DOCGOV DIRECTIVES -->
<!-- MANAGED BY DOCGOV: DO NOT EDIT THIS BLOCK OR ITS DELIMITERS. -->
## Documentation Standards <!-- @docgov-anchor -->

When creating or modifying documentation or architectural records:
1. **Taxonomy & Style**: Follow `docs/governance/documentation/core/taxonomy.md` for document categorization (Diátaxis) and `docs/governance/documentation/core/style.md` for technical writing voice.
2. **Architecture Decisions (`docs/adr/`)**: Author ADRs only for high-impact architectural changes (skip trivial refactorings). Follow `docs/governance/documentation/profiles/architecture/adr.md`, document rejected alternatives, and never revive superseded patterns.
<!-- END DOCGOV DIRECTIVES -->
