# docgov: Document Governance

A compiler-grade documentation and architecture governance standard and verification engine for software engineering repositories.

`docgov` unifies the governance standard (**spec**) and high-performance Rust verification toolchain (**cli**) into a single, cohesive monorepo with independent versioning.

---

## 1. Monorepo Architecture

```text
docgov/
├── Cargo.toml                  # Monorepo Cargo workspace
├── install.sh                  # Universal installation script
├── cli/                        # CLI engine package (docgov binary)
│   ├── Cargo.toml              # Rust crate manifest
│   ├── src/                    # Linter, sync engine, and verification rules
│   │   ├── bin/                # Entrypoints: `docgov` (and `dog` alias)
│   │   ├── rules/              # Compiler-grade invariant linter passes
│   │   ├── cli.rs              # CLI commands and lifecycle orchestrator
│   │   ├── remote.rs           # Upstream spec fetcher and release resolver
│   │   └── ...
│   └── tests/                  # Integration test suites
└── spec/                       # Sovereign specification standard
    ├── .manifest.json          # Protocol version metadata and SHA-256 asset catalog
    ├── default-config.yml      # Canonical adoption configuration template
    ├── directives.snippet      # Canonical AI agent directives block
    ├── core/                   # Core governance specification
    │   ├── taxonomy.md         # Multi-dimensional governance coordinates
    │   ├── invariants.md       # Codified system invariants ([INV-*])
    │   ├── style.md            # Technical voice & citation contracts
    │   └── workflow.md         # Unified operational lifecycle
    └── profiles/               # Vertical Domain Entities
        ├── architecture/       # ADR, Living Snapshot, RFC profiles
        ├── operations/         # Postmortem profiles
        └── validation/         # Universal acceptance & testing profiles
```

---

## 2. Independent Versioning & Tagging

`docgov` uses decoupled, independent semantic release tags for the CLI engine and the governance specification:

| Component | Tag Pattern | Target Artifacts | Description |
|---|---|---|---|
| **CLI** | `vx.x.x` | Multi-arch binaries (`docgov`), crates.io | High-performance verification and sync CLI |
| **Spec** | `spec-vx.x.x` | `docgov-spec-*.tar.gz`, `directives.snippet` | Sovereign documentation governance standard |

Release workflows are automated via GitHub Actions in `.github/workflows/`:
* Pushing a `v*` tag triggers cross-compilation and publishes binaries (`x86_64-linux`, `musl`, `macOS arm64/x86_64`, `Windows`).
* Pushing a `spec-v*` tag packages the specification bundle and updates release distribution assets.

---

## 3. Quick Start

### Installation

Install `docgov` via the universal installer:

```bash
# Installs docgov command
curl -fsSL https://raw.githubusercontent.com/ming2k/docgov/main/install.sh | sh
```

Or build from source:

```bash
# Clone the monorepo
git clone https://github.com/ming2k/docgov.git
cd docgov

# Build all workspace binaries
cargo build --release

# Binary is available at target/release/docgov
./target/release/docgov --help
```

### CI Integration

`docgov` ships as a single self-contained binary. Install it (via the
[universal installer](#installation) or `cargo install --path cli`) and invoke it
directly from any CI provider using the `--format` flag your platform understands
(`github`, `json`, or `text`):

```yaml
# GitHub Actions example
- name: Install docgov
  run: curl -fsSL https://raw.githubusercontent.com/ming2k/docgov/main/install.sh | sh

- name: Verify Documentation Governance
  run: docgov check --format=github
```

---

## 4. CLI Commands

```bash
# Print the engine version (e.g. docgov 0.0.6)
docgov --version

# Initialize standard .docgov.yml configuration and AGENTS.md in the workspace
docgov init

# Deterministic, offline, read-only CI verification of all repository invariants
docgov check

# Check, synchronize assets, and update the workspace to a newer specification release
docgov update

# Fast git-diff trigger verification (code-to-doc synchronization)
docgov diff
```

### Output Contract

All commands share one output contract, selectable with `--format` and `--color`:

| Format | Audience | Shape |
| :--- | :--- | :--- |
| `text` (default) | Humans | Report with distinct severity glyphs (`×` error, `!` warning), `help:` suggestions, and a summary verdict line |
| `github` | GitHub Actions | `::error file=…,line=…,col=…,title=[RULE]::message` workflow commands |
| `json` | Tooling | Versioned envelope: `{schema_version, protocol_version, summary, diagnostics}` |

`--color <auto\|always\|never>` defaults to `auto`, which honors the `NO_COLOR` convention
and colorizes only when stdout is a terminal. Machine formats (`github`, `json`) are pure
data — no color, timing, or advisory hints — so they can be redirected and parsed directly.
Set `DOCGOV_ASCII=1` to swap the Unicode glyph alphabet for 7-bit ASCII.

Exit codes are stable across commands: `0` clean, `1` findings or an available update,
`2` engine failure. See [ADR-0001](docs/adr/0001-cli-output-contract.md) for the binding
invariants.

---

## 5. Development & Testing

Run full test suites across the monorepo:

```bash
# Run unit and integration tests
cargo test --workspace

# Lint with clippy (zero warnings enforced in CI)
cargo clippy --workspace --all-targets -- -D warnings

# Verify formatting
cargo fmt --check --manifest-path cli/Cargo.toml
```

---

## License

MIT License. See [LICENSE.md](LICENSE.md) for details.
