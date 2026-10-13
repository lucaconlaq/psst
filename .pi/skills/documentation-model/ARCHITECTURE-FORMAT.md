# Architecture document format

## Location and purpose

Use `docs/ARCHITECTURE.md` as the architecture document. Describe the current system, grounded in verified code and configuration, rather than proposed changes. Create the file when there is content to write, not as an empty template.

Read `docs/GLOSSARY.md` before writing or editing. Use its canonical terms and meanings without duplicating definitions. Surface terminology conflicts and distinguish verified facts from uncertainty. Keep proposed architecture changes in the relevant idea or spec until implemented; architecture documentation is not a competing requirements contract or implementation authorization.

## Structure

Write in English using this structure:

```markdown
# Architecture

## Purpose
What this system does.

## Entry Points
CLI commands, TUI, server routes, jobs, workers, etc.

## Technology used
Language (or languages) of reference, main libraries, infrastructure.

## Main Modules
Major modules and their responsibilities.

## Module Boundaries / Seams
What each module owns, what interface it exposes, and what should not cross the seam.

## Data Flow
How data moves through the system.

## Storage / Persistence
Config files, databases, caches, local storage, external systems.

## External Integrations
APIs, agents, secret stores, third-party services.

## Testing Seams
Where behavior should be tested.

## Important Constraints
Security, performance, portability, compatibility.
```

Keep the document concise and specific to the system. Mark inapplicable sections explicitly rather than inventing infrastructure or integrations. Describe testing seams without claiming unverified test coverage. Surface discrepancies between documented constraints and actual behavior rather than silently treating intent as fact.

## Splitting large documents

If `docs/ARCHITECTURE.md` exceeds 500 lines, it may be split into topic files under `docs/architecture/*.md`, using short kebab-case filenames such as `module-boundaries.md` or `data-flow.md`.

Keep `docs/ARCHITECTURE.md` as the entry point, preserving the section headings with concise summaries and relative Markdown links to the extracted details, for example `[Data flow details](architecture/data-flow.md)`. Link every extracted file from this entry point. Move detail rather than duplicating it; each topic should have one authoritative home. Create the directory and topic files only when needed.
