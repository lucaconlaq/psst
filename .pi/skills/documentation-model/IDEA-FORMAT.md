# Idea document format

## Location and naming

Use `docs/ideas/` relative to the repository root. New ideas use `XX-name.md`: the next available numeric prefix (starting at `01`) and a short kebab-case name. Never overwrite an existing file. Keep an existing idea's path when grilling it.

## Terminology

Read `docs/GLOSSARY.md` before writing or editing. Use its canonical terms and meanings; do not duplicate definitions. Preserve ambiguous user wording as an explicitly unresolved question rather than silently assigning it a meaning. Check terminology before saving.

## Structure

Write in English. Every idea has a title, status, and short executive summary:

```markdown
# Idea title

Status: Parked

## Executive summary

Briefly describe the idea and its purpose.

## Notes

Details supplied by the user, including uncertainty.
```

Omit `Notes` if there are no additional details. Capture only what the user supplied; a parked idea needs no design tree or resolved requirements.

## Status and grilling sections

- `Parked`: captured, not yet being grilled.
- `Brainstorming`: grilling in progress or paused.
- `Agreed`: the user confirmed the shared understanding.

During grilling, preserve existing content and add or maintain these sections as needed:

- **Design tree**: stable question IDs, dependencies, decisions, and rationale. Distinguish recommendations from agreed decisions; mark excluded branches explicitly.
- **Open questions and investigations**: unresolved questions, recommendations, and facts still to investigate, so the session can resume.
- **Agreement**: the understanding explicitly confirmed by the user.

Record a concise working summary, not a conversation transcript. Never present unanswered recommendations as agreed decisions.
