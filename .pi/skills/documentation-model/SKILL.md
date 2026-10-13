---
name: documentation-model
description: Keep the project's rules, glossary, architecture, idea, spec, and ticket documents consistent. Use when defining domain terminology or creating, editing, or discussing document structures, statuses, or relationships.
---

# Documentation Model

Use lazy loading for this document structure: load only the formats needed for the current task, not every format upfront. Create files and directories lazily, only when there is content to write:

```text
AGENTS.md                       # Agent instructions and documentation links
README.md                       # User-facing introduction and usage
docs/
├── rules/
│   ├── index.md                # File-pattern table linking to applicable rules
│   └── RUST.md                 # Rust code and Cargo manifest rules, when applicable
├── GLOSSARY.md                  # Canonical domain vocabulary
├── ARCHITECTURE.md              # Current system architecture
├── architecture/*.md            # Optional detail files when architecture exceeds 500 lines
├── ideas/XX-name.md              # Parked → Brainstorming → Agreed
├── specs/XX-name.md              # Same filename; links to the agreed idea
└── tickets/<feature-slug>/NN-slug.md  # One vertical slice per file
```

Load the relevant format on demand: [glossary](GLOSSARY-FORMAT.md), [architecture](ARCHITECTURE-FORMAT.md), [ideas](IDEA-FORMAT.md), [specs](SPEC-FORMAT.md), or [tickets](TICKET-FORMAT.md). These files are the authoritative naming, structure, and content rules.

## Documentation entry points

Root `AGENTS.md` must link to `docs/ARCHITECTURE.md`, `docs/GLOSSARY.md`, `README.md`, and `docs/rules/index.md`, explaining when to read each.

`README.md` is for users to understand the library or tool: what it does, how to install it, and how to use it. Keep internal architecture in `docs/ARCHITECTURE.md` and canonical domain definitions in `docs/GLOSSARY.md`; link to them rather than duplicating their content.

## Rules

Keep coding and file-specific conventions in `docs/rules/`, separate from domain definitions and architecture. `docs/rules/index.md` must contain a table mapping file patterns to linked rule documents and their scope. Patterns apply throughout the repository; before changing or reviewing files, consult the index and read and follow all matching rules. Load only applicable rule documents.

For Rust projects, map `*.rs` to `RUST.md`. Map `Cargo.toml` to the same document when it includes Cargo manifest conventions. Preserve existing guidelines when moving them into `docs/rules/`; update references rather than maintaining duplicate sources of truth. Create additional rule documents and table entries only when there are rules to write.

## Domain language

Read `docs/GLOSSARY.md` when discussing domain concepts; use its canonical terms in code, documentation, and discussions. Keep a reference to it in root `AGENTS.md`.

Challenge vague or conflicting terms, test distinctions with concrete edge cases, and check claims against existing code. Surface contradictions for resolution rather than silently redefining terms. Once a term is resolved through verified existing behavior or explicit user agreement, add or revise its definition immediately. This is a narrow exception to workflows that otherwise restrict edits to their target idea, spec, or tickets; it does not authorize changes to other documents or project code. Keep unresolved proposals in the relevant idea or spec, not the glossary. If spec or ticket generation requires an unresolved terminology decision, stop and report the blocker rather than inventing agreement. The glossary contains domain definitions, never implementation details or requirements.

## Document consistency

Before creating or editing architecture documentation or any idea, spec, or ticket, read `docs/GLOSSARY.md`. Use its canonical terms and meanings throughout, including titles and acceptance criteria. Check terminology before saving; avoid aliases and conflicting local definitions. Do not duplicate glossary definitions in these documents. Flag unresolved terminology without inventing an agreed meaning; update the glossary once resolved, then align only documents authorized by the current workflow. Report inconsistencies in other affected documents rather than editing them; never change their requirements or approval status without the required request and agreement.

Preserve uncertainty and distinguish proposals, verified facts, and user-confirmed decisions. Surface contradictions instead of silently resolving them. Architecture documents describe the verified current system and its boundaries; ideas retain decision history and agreement; specs own the current behavioral contract; tickets describe execution and verification for a slice, referencing the source contract. Concise repetition for context or self-contained tickets is allowed, but must not create a competing definition. Retain source links and flag conflicts rather than silently resolving them.

Use `add-idea`, `grill-idea`, `to-spec`, and `to-tickets` for their respective workflows and approval gates. Never advance status without the required agreement, overwrite without an explicit update request, or treat documentation as implementation authorization.
