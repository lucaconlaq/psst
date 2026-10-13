---
name: to-spec
description: Convert an agreed, grilled idea in docs/ideas/ into a local spec in docs/specs/. Use when the user asks to turn their current grilled idea into a spec; synthesize without interviewing or implementing.
---

Read [documentation-model](../documentation-model/SKILL.md), [SPEC-FORMAT.md](../documentation-model/SPEC-FORMAT.md) for the spec format and naming rules, and [IDEA-FORMAT.md](../documentation-model/IDEA-FORMAT.md) for statuses and agreement semantics.

## Source and readiness

Read the supplied idea or the one clearly identified by the current conversation in `docs/ideas/`. If there is no clear match, stop and ask the user to provide the idea path; do not guess or start a design interview. If no idea exists, direct the user to [add-idea](../add-idea/SKILL.md), then [grill-idea](../grill-idea/SKILL.md).

Require `Status: Agreed`, an explicit user-confirmed agreement, and no unresolved design prerequisites. If these are missing, report what blocks conversion and direct the user to resume `grill-idea`; do not grill through this skill. Explicitly delegated implementation details and future release verification are not unresolved design prerequisites.

Announce the source path. Treat its agreement and settled decisions as authoritative. Use conversation context to clarify them, not silently override them. Preserve exclusions, rationale, risks, acceptance requirements, and bounded implementation discretion. Distinguish historical proposals and superseded recommendations from the final agreement.

## Synthesis

1. Inspect the relevant current code, tests, project documentation. Reuse verified repository understanding where still current.
2. Identify the highest existing testing seams that can verify the agreed behavior. Prefer a small number of shared seams, ideally one where adequate, but retain lower-level coverage required by the agreement. Describe any necessary new seam as a proposal, not a user-approved decision. Do not ask for seam confirmation or conduct any interview.
3. Write the spec using `SPEC-FORMAT.md`. Cover the entire agreed scope without padding the user stories or inventing features. Make success, failure, edge cases, compatibility, and release gates testable. Separate agreed decisions, verified current-state facts, and proposed testing details. If repository findings materially conflict with the agreement, stop and report the conflict instead of choosing a new design.
4. Check the spec against every settled design branch and explicit exclusion. Ensure no recommendation became an agreement, no delegated detail became a newly settled design choice, and no future verification is presented as already completed.
5. Perform a deduplication pass using SPEC-FORMAT.md's requirement-ownership rules. Consolidate overlapping stories and bullets, keep exact contracts in one authoritative section, and replace necessary repetitions with brief references. Remove discussion recaps and notes already covered elsewhere. Recheck agreement coverage after cutting: remove duplicate wording, never unique requirements, rationale, risks, exclusions, or test obligations.
6. Create only the spec document, except for glossary updates allowed by `documentation-model`. If its destination already exists, stop and report that path rather than overwriting it; updating an existing spec requires an explicit request. Leave the source idea, its status, and project code unchanged.

Report the resulting spec path and stop. Do not publish issues, apply labels, implement the feature, or treat spec creation as implementation authorization.
