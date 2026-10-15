# Architecture Decision Records

| ID | Title | Status | Scope | Decision Summary & Primary Invariant | Date |
| :--- | :--- | :--- | :--- | :--- | :--- |
| [0001](0001-cli-output-contract.md) | Unified CLI Output Presentation Contract | Accepted | cli/presentation | All terminal symbols, color, and severity formatting flow through `cli/src/output.rs`; emoji are prohibited, color is auto-gated (never unconditional), and `--format=json` is a versioned envelope rather than a bare array | 2026-02-14 |
| [0002](0002-authority-ssot-and-directives-modernization.md) | Sole Declarative Authority in .docgov.yml, Directives Modernization, and Bifurcated Enforcement | Accepted | cli/governance/spec | `.docgov.yml` is the sole declarative source of truth; bifurcated enforcement model strictly defends managed zones (Error) while providing permissive advisories in user space (Warning); retires INV-LINT-02 under Invariant Deprecation SOP | 2026-03-31 |
