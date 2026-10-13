---
name: documentation-model-init
description: Audit a new repository against the documentation model and propose a non-destructive migration plan. Run explicitly with /skill:documentation-model-init.
disable-model-invocation: true
---

# Initialize the documentation model

Audit the current repository and propose steps to adopt the documentation model. This is a read-only assessment, not permission to initialize files or execute the migration. Do not create, edit, move, or delete repository files, change statuses, or implement code—even if command arguments request it. Implementation requires a separate follow-up request.

## Load the model

Read [the documentation model](../documentation-model/SKILL.md). Resolve this link relative to this skill directory, not the repository being audited. Read its linked format files only for document types present in the repository or relevant to the proposed migration. If the model or a required format is unavailable, report the missing dependency rather than inventing rules. When distributing this skill to another repository, include the sibling `documentation-model` skill and its format files.

## Inspect the repository

1. Identify the repository root and read its applicable agent instructions. Inventory existing documentation, including files outside `docs/`; exclude dependencies, generated output, and vendored documentation. Inspect source and configuration only as needed to verify architectural claims and document purpose. Do not run project code or installation commands.
2. Check root `AGENTS.md` and `README.md`:
   - Both are present and have useful content, not just placeholders.
   - `AGENTS.md` links to `docs/ARCHITECTURE.md`, `docs/GLOSSARY.md`, root `README.md`, and `docs/rules/index.md`, with guidance on when to read each.
   - `README.md` helps users understand the library or tool: purpose, installation, and usage. Internal architecture and canonical domain definitions belong in their dedicated documents; link to them when relevant rather than duplicating them.
   - Required links resolve to the intended documents. Check existing documentation links in both files for missing files and invalid local anchors. Do not require README links beyond those required by the model or label unchecked external URLs as verified.
3. Check `docs/GLOSSARY.md` and `docs/ARCHITECTURE.md` against their formats. Distinguish verified current architecture from proposals, and flag terminology conflicts, duplicated definitions, stale claims, and missing boundaries. Missing documents are gaps only when there is content to write: do not propose empty templates or invented terminology for an empty repository. Report required AGENTS links whose targets cannot yet be populated as deferred, not satisfied.
4. If ideas, specs, or tickets exist, check their locations, naming, structure, statuses, source relationships, and ownership of requirements against the applicable formats. Inspect legacy equivalents too, and propose where their content belongs. Do not infer user agreement from a finished-looking document or fabricate source ideas for existing specs. Preserve uncertainty and approval history.
5. Check `docs/rules/index.md` for a table mapping repository-wide file patterns to linked rule documents and their scope. Verify that the links resolve and existing coding guidelines are preserved without duplicate sources of truth.
6. Check optional architecture detail files for links from the architecture entry point. Respect lazy creation: absent idea, spec, ticket, or architecture-detail directories are not violations when there is no content for them.

## Report and propose

Return a concise assessment with:

- **Verdict:** aligned, partially aligned, or not yet initialized; state any inspection limits.
- **Findings:** concrete paths and evidence, separating violations from optional improvements and unresolved questions. Identify missing or incorrect AGENTS/README links explicitly.
- **Migration plan:** ordered, minimal steps with source → target paths for moves or splits, required link updates, and checks to perform afterward. Preserve useful content, existing instructions, provenance, and unique requirements. Flag conflicts and destructive changes for explicit user decisions; do not silently choose new definitions or advance statuses.

Prefer adapting existing documents over rewriting them or creating duplicate sources of truth. Include only necessary steps; if the repository already conforms, say so rather than inventing refactoring work. Stop after presenting the plan—do not apply it or begin an interview unless a blocking ambiguity prevents a meaningful assessment.
