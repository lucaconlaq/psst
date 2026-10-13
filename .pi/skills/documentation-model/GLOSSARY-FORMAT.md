# Glossary format

## Location and purpose

Use `docs/GLOSSARY.md` as the canonical domain vocabulary. Reference it from root `AGENTS.md`. Create it when the first term is resolved, not as an empty template.

## Structure

```markdown
# Project Glossary

Canonical domain vocabulary for code, documentation, and discussions.

## Terms

**Entry**:
A named configuration item that associates an environment name with a provider and a stored value.
_Avoid_: Secret (when referring to the configuration item rather than its resolved value)
```

## Rules

- Add only understood, resolved terms grounded in existing behavior or user-confirmed decisions; do not turn proposals into facts.
- Pick one canonical term per concept. List confusing alternatives under `_Avoid_` when useful, qualifying context rather than banning valid distinct terms.
- Define what the concept is in one or two sentences. Prefer existing glossary terms within definitions.
- State project-specific meanings and distinctions explicitly. Group terms under subheadings only when useful.
- Update definitions in place as understanding changes; surface conflicts before changing agreed meanings.
- Include only domain language, not implementation details, requirements, workflow statuses, or design decisions.

Adapted from [Glossary Format](https://github.com/mattpocock/skills/blob/main/skills/productivity/teach/GLOSSARY-FORMAT.md) and [Domain Modeling](https://github.com/mattpocock/skills/blob/main/skills/engineering/domain-modeling/SKILL.md).
