# Documentation Governance Core

Universal documentation governance standards and architectural guidelines for software engineering repositories.

---

## Core Documentation Standards

Documentation governance is organized around two foundational standards and domain capability profiles:

| Core Document | Standard | Description |
| :--- | :--- | :--- |
| [Taxonomy](taxonomy.md) | **Governance Coordinates** | 4-Dimensional system: Temperature, Lifecycle, Audience Layering, Diátaxis |
| [Style Guide](style.md) | **Syntax & Formatting** | Voice, capitalization, heading hierarchy, relative links, and document patterns |

---

## Domain Capability Profiles

Repositories activate domain capabilities declared in `.docgov.yml`. Each profile provides vertical entities and authoritative templates:

| Domain Profile | Focus | Vertical Entities | Location |
| :--- | :--- | :--- | :--- |
| **Architecture** | Architectural decision records, living blueprints, and proposals | `adr.md`, `living-snapshot.md`, `rfc.md` | `profiles/architecture/` |
| **Validation** | Real user journey acceptance and programmatic test verification | `acceptance.md`, `testing.md` | `profiles/validation/` |
| **Operations** | Operational knowledge layering and incident analysis | `postmortem.md` | `profiles/operations/` |

---

## Quick Navigation

1. **Deciding where content lives**: Consult [Taxonomy](taxonomy.md).
2. **Formatting and technical voice**: Review the [Style Guide](style.md).
3. **Drafting an ADR**: Follow [Architectural Decision Records](profiles/architecture/adr.md).
