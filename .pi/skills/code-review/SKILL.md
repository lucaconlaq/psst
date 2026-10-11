---
name: code-review
description: Review Wumpa code file by file for correctness and compliance with docs/CODE_GUIDELINES.md. Use when asked for a code review, an audit, or a guideline compliance check. Always begin by asking whether to review uncommitted edits, a specific file or folder, or the whole project. Report findings without modifying code.
---

# Code review

## 1. Ask what to review

Your first action must be `ask_user_question`. Ask **"What should I review?"**
with exactly these three options, single-select:

1. **Edits (uncommitted)** — Review staged, unstaged, and untracked changes.
2. **Specific folder or file** — Review all relevant content at a chosen path.
3. **All the project** — Review the entire project's maintained files.

Do not start reviewing or assume a scope before the user answers. If the user
cancels, stop.

For a specific file or folder, use a path already supplied by the user. If none
was supplied, inspect the available paths, then ask a follow-up using
`ask_user_question` with 2–4 actual file/folder candidates. The user may enter
another path through the tool's built-in custom answer. Never invent an "Other"
or "Type something" option. Resolve and validate the chosen path before proceeding.

## 2. Read the project's rules

Find the repository root. Paths below are relative to that root, **not** this
skill's directory.

Read these files completely before reviewing:

- Applicable `AGENTS.md` instructions, including nested ones for selected files.
- `README.md` for intended behavior and known prototype limitations.
- `docs/CODE_GUIDELINES.md` for conventions and engineering requirements.

If the guidelines are missing or unreadable, report the problem and ask for the
correct location. Do not claim guideline compliance without reading them.
Do not fetch every external style-guide link by default; consult a linked source
only when necessary to resolve a concrete uncertainty.

## 3. Build and show the file inventory

Inspect Git status without changing the index or working tree. Handle filenames
safely: use NUL-delimited Git output, quote paths, and use `--` before pathspecs.

### Edits (uncommitted)

Collect and deduplicate paths from:

```sh
git diff --name-status -z
git diff --cached --name-status -z
git ls-files --others --exclude-standard -z
```

Include staged changes, unstaged changes, and non-ignored untracked files.
Inspect both staged and unstaged diffs; do not use only `git diff HEAD`, since
staged changes can be undone in the working tree and disappear from that diff.
Handle added, deleted, renamed, and conflicted files explicitly. For a file with
both staged and unstaged changes, distinguish the staged snapshot from the
working-tree version when a finding depends on that distinction.

Read the complete current file for context, but prioritize issues introduced or
exposed by the edits. Clearly label any pre-existing issue rather than attributing
it to the change. For deleted files, inspect the previous content and references
to assess the deletion; do not try to read a nonexistent working-tree file.
If there are no uncommitted changes, say so and stop.

### Specific folder or file

Review the entire selected file, or recursively inventory maintained files under
the selected folder. Include relevant non-ignored untracked files. Do not silently
expand the review to unrelated folders. Read dependencies and callers outside the
scope as supporting context when needed, and label that context separately.

### All the project

Inventory tracked and non-ignored untracked files throughout the repository.
Include source, tests, manifests, configuration, scripts, and documentation.
Do not restrict discovery to `src/` or `*.rs`.

### Show the plan before reviewing

Print a numbered checklist of **every file selected for review**, with its Git
status when relevant. Sort it in a stable, sensible order. Include deleted paths.

Exclude build artifacts, caches, `.git/`, vendored dependencies, and generated
outputs from line-by-line source review. Identify excluded files/groups and why.
Treat lockfiles as dependency metadata: check relevant dependency/version changes
rather than applying handwritten Rust style rules. Mark binaries as not manually
reviewable. Do not expose secrets; report sensitive-file concerns without quoting
secret values. Never silently skip a maintained file just because it is large.

For a large inventory, show it in chunks and work through it in batches. Keep a
remaining-files checklist; do not claim a complete review if any files remain.

## 4. Review one file at a time

Follow the inventory **sequentially**. For each file:

1. Announce `Reviewing i/N: path`.
2. Read all of its relevant content with `read`, continuing with offset/limit
   when truncated. For edit reviews, also inspect the relevant diffs and previous
   versions. Follow callers, types, and tests as needed to verify behavior.
3. Check **correctness**:
   - Intended behavior, edge cases, validation, and state transitions.
   - Error propagation, panics, resource cleanup, and partial failures.
   - Persistence, atomicity, concurrency, cancellation, and timeouts.
   - Security boundaries, shell arguments, filesystem paths, and untrusted data.
   - Platform assumptions, Rust minimum-version compatibility, and dependencies.
   - Regression risks, test coverage, and agreement with documentation.
4. Check **`docs/CODE_GUIDELINES.md` compliance**:
   - Naming, module/import organization, formatting, and manifest layout.
   - Comments, documentation, attributes, and unsafe-code explanations.
   - Error handling, dependency choices, and repository engineering rules.
   - Edition/minimum-version policy and the manual review checklist.
   Apply Rust rules to Rust and manifest rules to manifests; mark irrelevant
   checks not applicable instead of inventing requirements for other file types.
5. Record the per-file result, then move to the next file. Track correctness and
   guideline compliance separately: `no issue found`, `findings`, or `not verified`.

Do not equate a passing formatter or compiler with correct code. Do not present
personal preferences as guideline violations. Cite the relevant guideline section
for compliance findings. Distinguish confirmed defects, potential risks requiring
verification, and optional suggestions.

## 5. Validate without changing code

Use the repository's pinned toolchain and documented commands. For Wumpa:

```sh
mise exec -- cargo fmt --check
mise exec -- cargo clippy --locked --all-targets -- -D warnings
mise exec -- cargo test --locked
```

Read test/build scripts before running them. Prefer isolated local tests; do not
contact real servers, alter user configuration, or run destructive checks without
permission. Do not run `cargo fmt` without `--check`, apply fixes, regenerate
lockfiles, stage files, commit, or push as part of a review.

When minimum-version compatibility is relevant, test with the declared minimum
Rust toolchain if available. Otherwise report that it remains unverified. Do not
install tools or change the pinned version without permission.

Record the exact commands, results, and any unavailable checks. A tool failure is
not automatically a code defect: distinguish environment problems from findings.
These commands generally test the working tree, not a separately staged snapshot;
state that limitation when relevant.

## 6. Report

Lead with actionable findings, highest severity first. For each finding include:

- Severity: **High**, **Medium**, or **Low**.
- Category: **Correctness** or **Guidelines**.
- A precise `path:line` reference (and snapshot if not the working tree).
- The issue, its impact, and evidence or triggering scenario.
- A concise proposed correction, without applying it.
- For guideline violations, the applicable section of `docs/CODE_GUIDELINES.md`.

Then give a compact per-file summary table:

| File | Correctness | Guidelines | Notes |
| --- | --- | --- | --- |

Finish with validation results, reviewed/total file count, skipped files, and
remaining uncertainties. If no actionable issues were found, say **"No issues
found in the reviewed scope"**, not that correctness is proven. Report incomplete
coverage honestly. Leave all source, configuration, and Git staging unchanged.
