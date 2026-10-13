---
name: implement
description: Implement a spec through its tickets in a dedicated worktree, using a fresh Pi process per ticket and automatic implementation and rules review fixes. Use when asked to implement a spec or its ticket set.
---

# Implement

Keep the parent agent an orchestrator. Do not implement tickets in its context. Use ordinary Git and the Pi CLI; no extension, SDK, or runner script is needed.

This workflow authorizes implementation, ticket lifecycle updates, and immediate fixes from both review skills. Review scopes supplied by this workflow are preauthorized: do not ask for scope or fix confirmation. Stop for ambiguous requirements, conflicting instructions, or unsafe operations rather than guessing.

## 1. Select and check

- Use the supplied spec in `docs/specs/`. Otherwise, inspect specs and their linked ticket sets: a spec is pending if it has no tickets or any ticket is not `done`. Default to the only pending spec; if there are several, use `ask_user_question` for a quick selection (offer a typed path if needed). If none are pending, report that and stop.
- Read the selected spec and [documentation-model](../documentation-model/SKILL.md), including [TICKET-FORMAT.md](../documentation-model/TICKET-FORMAT.md). Repository paths below are relative to the repository root.
- Find tickets under `docs/tickets/` by their source links, not just directory names. Read the complete set. Require defined acceptance criteria and verification, valid dependencies, and coverage of the spec. If tickets are missing or incomplete, stop and suggest `/skill:to-tickets`; do not create them here.
- Check that `pi` is available and that the spec, tickets, project instructions, and review skills exist at the current `HEAD`. Uncommitted versions will not appear in a new worktree: stop if required inputs differ from `HEAD`, without committing or copying them automatically. Leave unrelated source-worktree changes alone.

## 2. Isolate the work

Record the repository root and current `HEAD` as the base commit. Choose an unused branch `implement/<spec-stem>` and sibling worktree `<repo-name>-implement-<spec-stem>`; add a numeric suffix if needed. Never reuse or overwrite an existing branch or directory.

```sh
git worktree add -b "$branch" "$worktree" "$base"
```

Announce the spec, base commit, branch, worktree, and ticket order. Run all implementation and review processes with the worktree as their working directory. Do not modify the original worktree, commit, push, merge, or remove the new worktree automatically. Keep changes uncommitted so the review skills' default edited-file scope contains the accumulated implementation.

## 3. One fresh process per ticket

Process tickets sequentially in dependency order, breaking ties by ticket number. Skip verified `done` tickets. Start a ticket only when all its blockers are `done`; stop if there is no executable ticket while unfinished tickets remain.

For **each ticket**, launch a new process and wait for it to exit:

```sh
(cd "$worktree" && pi --approve --print --no-session "$prompt")
```

`--approve` trusts this project's resources for the child process. Do not use `--continue`, `--resume`, `--session`, or `--fork`. Do not pass the parent transcript, previous child conversations, or implementation dumps. A new process with only a short task prompt is the context reset; compaction or an instruction to forget is not a substitute.

Construct the prompt with the spec path, ticket path, base commit, and these instructions:

1. Read `AGENTS.md`, the ticket and its source spec, and applicable project documentation and rules. Read `.pi/skills/implement/SKILL.md` for workflow authorization only; do not recursively orchestrate or launch children. Reconstruct needed context from repository files.
2. Mark the ticket `in-progress`; implement only its requirements and necessary tests. Preserve earlier tickets' work. Run its required verification with the documented toolchain.
3. Load `.pi/skills/review-implementation/SKILL.md`, review against this ticket, and apply findings immediately without confirmation. Use the default edited-file scope; distinguish earlier tickets' required work from scope creep in this ticket.
4. Load `.pi/skills/review-rules/SKILL.md`, review the edited files, and apply findings immediately without confirmation. Rerun relevant checks after fixes; recheck affected requirements if rules fixes change behavior.
5. Only mark the ticket `done` and check its criteria when implementation, both reviews, and required verification succeed. Record concise verification evidence in the ticket and update newly unblocked tickets according to the lifecycle rules. Otherwise leave it unfinished and report the blocker.
6. Do not commit, stage, push, merge, or alter source requirements. Return a brief summary: ticket status, changed paths, checks, review fixes, and unresolved findings.

The parent reads the child's exit status, summary, and ticket evidence before advancing. A zero exit status alone is not proof of completion. Stop on failure, incomplete verification, unresolved findings, or an unfinished ticket; report the worktree and blocker. Do not blindly retry or continue with dependents.

## 4. Final spec reviews

After every ticket is `done`, launch one more fresh Pi process with the same invocation. Supply only the spec path, ticket-set path, base commit, and these instructions:

- Read project instructions, the complete spec, and ticket verification evidence.
- Load `review-implementation` and review the complete spec implementation, including reused existing code and all accumulated edits. Resolve and announce that concrete scope; it is preauthorized. Check cross-ticket integration and all spec requirements, not just ticket checkboxes. Apply corrections and necessary regression tests without confirmation.
- Load `review-rules` and review all accumulated edited files, applying corrections without confirmation. Rerun required spec-level checks and recheck behavior affected by fixes. Never change requirements to fit the implementation.
- Reconcile affected ticket statuses and evidence; do not leave `done` claims for unmet criteria. Do not launch children, commit, stage, push, or merge. Report reviewed coverage, fixes, checks, and unresolved findings.

Report completion only when all tickets remain verified `done`, both final reviews are complete, and required checks pass with no unresolved findings. Otherwise report partial completion and blockers. End with the branch, worktree path, and concise verification summary; leave the worktree available for inspection.
