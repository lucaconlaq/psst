# Spec document format

## Location and naming

Use `docs/specs/` relative to the repository root, creating the directory when needed. Keep the source idea's filename: `docs/ideas/XX-name.md` becomes `docs/specs/XX-name.md`. Never overwrite an existing spec without an explicit update request.

Write in English. Link back to the source idea with a relative Markdown link. Source provenance is the exception to the restriction on implementation file paths below.

## Structure

```markdown
# Feature title

Source idea: [Idea title](../ideas/XX-name.md)

## Problem Statement

Describe the problem from the user's perspective and why it matters.

## Solution

Describe the agreed solution from the user's perspective, including its boundaries.

## User Stories

1. As an <actor>, I want <feature>, so that <benefit>.

## Implementation Decisions

- Record the agreed technical decisions and their important rationale.

## Testing Decisions

- Describe observable behavior, testing seams, and acceptance coverage.

## Out of Scope

- Record explicit exclusions and non-goals.

## Further Notes

Record remaining contextual notes, risks, implementation discretion, and release verification where relevant. Use “None” if there are no further notes.
```

## Content rules

### Terminology

Read `docs/GLOSSARY.md` before writing or editing. Use its canonical terms and meanings throughout, including user stories and contracts; do not duplicate definitions. Surface conflicts with the source idea instead of silently changing agreed meaning. Resolve new terms in the glossary before treating them as agreed contracts, and check terminology before saving.

### Concision and requirement ownership

Synthesize the agreement; do not reproduce the design discussion or restate each requirement in every section. The spec owns the current behavioral contract; the source idea retains decision history and agreement. Give each exact requirement one authoritative home within the spec:

- Problem Statement and Solution: brief motivation and high-level approach.
- User Stories: distinct user goals and benefits.
- Implementation Decisions: precise behavioral and technical contracts.
- Testing Decisions: how those contracts will be verified and the release gates.
- Out of Scope: explicit exclusions.
- Further Notes: context or risks not already covered elsewhere.

Other sections may briefly summarize or refer to an authoritative requirement when needed for understanding, but must not repeat its full constraints. Tickets may concisely repeat relevant constraints for self-contained execution and verification, referencing this contract without redefining it. Consolidate overlapping bullets and stories; omit historical question-by-question recaps and redundant notes. Link to the source idea for decision history. Preserve every unique agreed constraint, rationale, risk, and exclusion; brevity must not weaken acceptance criteria or erase important distinctions.

### User Stories

Use a concise numbered list in the form `As an <actor>, I want <feature>, so that <benefit>`. Group related behavior into distinct user goals rather than creating one story per technical constraint, error case, or test. The spec as a whole must cover the entire agreement; the story list need not duplicate that coverage. Keep exact safety, compatibility, and operational constraints in Implementation Decisions. Add concise observable acceptance criteria beneath a story only when needed and not already specified elsewhere.

### Implementation Decisions

Describe modules and responsibilities, interfaces, architectural decisions, schema and API contracts, important interactions, persistence, and security boundaries as applicable. Preserve exact agreed names, formats, limits, ordering, and failure semantics.

Do not include specific implementation file paths or ordinary code snippets: they become stale. Persisted artifact locations and public configuration names that are part of the agreed contract are allowed. If an existing prototype snippet encodes a decision more precisely than prose (for example a state machine or schema), include only the decision-rich portion and identify its prototype origin; do not invent a prototype or include a working demo.

Identify current codebase facts separately from agreed changes. Keep explicitly delegated details delegated, with their constraints; do not choose libraries, schemas, or UI layouts beyond what the user agreed.

### Testing Decisions

Specify tests of externally observable behavior, not private implementation details. Name the modules or boundaries exercised and describe relevant existing test patterns without implementation file paths. Prefer the highest existing seam and the fewest seams that adequately cover the agreement. Clearly label new seams or additional test strategies as proposals when not previously agreed.

Cover successful behavior, failures, edge cases, and regressions required by the agreement. Group related cases by observable boundary and refer to the relevant decision instead of repeating its complete contract. Keep testing-specific details here. State release gates and any opt-in integration checks explicitly. Distinguish planned verification from evidence already obtained; a prototype is not a completed release test or independent review.

### Out of Scope and Further Notes

Preserve explicit exclusions, accepted limitations, threat boundaries, recovery obligations, and deferred work. Do not reintroduce excluded branches as user stories or implied requirements. Record bounded implementation discretion and future verification without inventing open product decisions. Spec creation does not authorize implementation.
