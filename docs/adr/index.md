# Architecture Decision Records

| ID | Title | Status | Scope | Decision Summary & Primary Invariant | Date |
| :--- | :--- | :--- | :--- | :--- | :--- |
| [0001](0001-cli-output-contract.md) | Unified CLI Output Presentation Contract | Accepted | cli/presentation | All terminal symbols, color, and severity formatting flow through `cli/src/output.rs`; emoji are prohibited, color is auto-gated (never unconditional), and `--format=json` is a versioned envelope rather than a bare array | 2026-02-14 |
