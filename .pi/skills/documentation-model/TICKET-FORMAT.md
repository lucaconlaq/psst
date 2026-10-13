# Ticket document format

## Location and naming

Use `docs/tickets/<feature-slug>/` relative to the repository root. Write one file per ticket, never a combined file. Name files `<NN>-<slug>.md`, numbered from `01` in dependency order (blockers first), with short kebab-case slugs. When extending an existing ticket set, inspect its tickets and continue after the highest existing number; never reuse numbers or renumber existing tickets. Never overwrite existing tickets without an explicit update request.

## Template

```markdown
# <NN>: <Ticket title>

**Source:** <link to spec or plan; omit if conversation-only>

**What to build:** <end-to-end behavior from the user's perspective>

**Blocked by:** <numbers/titles, or "None (can start immediately)">

**Status:** <blocked or ready-for-agent; see lifecycle rules below>

- [ ] <Observable acceptance criterion>
- [ ] <Required verification>
```

## Lifecycle

Status records executability and progress, not approval or implementation authorization. Allowed statuses:

- `blocked`: at least one listed blocker is not `done`.
- `ready-for-agent`: all listed blockers are `done`, or there are no blockers; work has not started.
- `in-progress`: authorized implementation has started with no outstanding blockers.
- `done`: all acceptance criteria and required verification are satisfied, with their checkboxes checked and verification evidence recorded in the ticket.

Create tickets as `blocked` or `ready-for-agent` according to their dependencies. Record blocker completion by marking the blocker ticket `done`; retain dependent tickets' blocker references for traceability. When all blockers are `done`, move an unstarted dependent ticket from `blocked` to `ready-for-agent`. Move it to `in-progress` only when authorized work starts, and to `done` only after completion is verified. If a blocker is reopened, reassess affected dependents rather than leaving stale readiness or completion claims. Update statuses only within the authorized workflow; report needed changes outside its scope.

## Content rules

Read `docs/GLOSSARY.md` before writing or editing. Use its canonical terms and meanings throughout, including titles and acceptance criteria; do not duplicate definitions. Surface terminology conflicts with the source spec rather than redefining terms locally. Resolve new terms in the glossary before treating them as acceptance criteria, and check terminology before saving.

Write in English. Keep each ticket self-contained and concise, describing user-visible behavior rather than a layer-by-layer implementation list. Include observable acceptance criteria and required verification, preserving relevant agreed constraints, failures, compatibility, safety, and release gates. Tickets describe execution and verification for a slice, not a competing behavioral contract. Concisely repeat relevant source constraints when needed for self-contained execution, retaining source links without redefining the contract. Flag conflicts with the source rather than silently resolving them.

List only genuine blockers, by ticket number and title. No outstanding blockers means the ticket is executable, but does not authorize implementation.

Avoid implementation file paths and ordinary code snippets. Source links and persisted artifact locations that are agreed contracts are allowed. Only inline a decision-rich prototype snippet when more precise than prose, noting its origin.
