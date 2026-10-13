---
name: implement
description: Implement a spec through its tickets in a dedicated worktree, using fresh implementation processes, independent parallel spec and rules reviewers, automatic fixes, and a commit for each verified ticket. Use when asked to implement a spec or its ticket set.
---

# Implement

Keep the parent agent an orchestrator. Do not implement tickets in its context. Use ordinary Git for worktrees and the project-local `.pi/extensions/subagent.ts` tools for child processes. Do not generate launcher or monitoring scripts.

This workflow authorizes implementation, ticket lifecycle updates, automatic review fixes, and parent-created commits for verified tickets without additional confirmation. Follow the message conventions in [commit](../commit/SKILL.md); this workflow overrides that skill's per-commit confirmation requirement. Review scopes supplied by this workflow are preauthorized: do not ask for scope or fix confirmation. Review children are explicitly report-only; the fix child applies their findings afterward. This overrides the review skills' usual immediate-fix exception for `implement`. Stop for ambiguous requirements, conflicting instructions, or unsafe operations rather than guessing.

Use two-axis review: spec correctness and coding standards get independent parallel reviewers, isolated from implementation and from each other. Keep their findings separate; passing one axis never compensates for failing the other. Use this project's review skills and rules, not an additional generic smell baseline.

## 1. Select and check

- Use the supplied spec in `docs/specs/`. Otherwise, inspect specs and their linked ticket sets: a spec is pending if it has no tickets or any ticket is not `done`. Default to the only pending spec; if there are several, use `ask_user_question` for a quick selection (offer a typed path if needed). If none are pending, report that and stop.
- Read the selected spec and [documentation-model](../documentation-model/SKILL.md), including [TICKET-FORMAT.md](../documentation-model/TICKET-FORMAT.md). Repository paths below are relative to the repository root.
- Find tickets under `docs/tickets/` by their source links, not just directory names. Read the complete set. Require defined acceptance criteria and verification, valid dependencies, and coverage of the spec. If tickets are missing or incomplete, stop and suggest `/skill:to-tickets`; do not create them here.
- Check that `pi`, GNU `timeout`, and `ps` are available on Linux, and that `subagent_start`, `subagent_wait`, and `subagent_cancel` are loaded (use `/reload` after installing the extension). Check that the spec, tickets, project instructions, review skills, and `.pi/extensions/subagent.ts` exist at the current `HEAD`. Uncommitted versions will not appear in a new worktree: stop if required inputs differ from `HEAD`, without committing or copying them automatically. Leave unrelated source-worktree changes alone.

## 2. Isolate the work

Record the repository root and current `HEAD` as the base commit. Use `../worktrees` relative to the repository root as the worktree directory. Choose an unused worktree name `<repo-name>-implement-<spec-stem>` and branch `implement/<spec-stem>`; add the same numeric suffix to both if either is taken. Never reuse or overwrite an existing branch or directory.

Create the worktree detached first, then create its dedicated branch:

```sh
mkdir -p "$worktree_directory"
worktree="$worktree_directory/$worktree_name"
git worktree add --detach "$worktree" "$base"
git -C "$worktree" switch -c "$branch"
```

Announce the spec, base commit, branch, worktree, and ticket order. Run all implementation and review processes with the worktree as their working directory. Do not modify the original worktree, push, merge, or remove the new worktree automatically. Commit each verified ticket on the dedicated branch as described below. Keep the original base commit fixed for every review so committed tickets remain in the accumulated implementation scope.

## 3. Fresh implementation process per ticket

Process tickets sequentially in dependency order, breaking ties by ticket number. Skip verified, committed `done` tickets. If a completed ticket lacks its commit, reconcile that checkpoint before advancing. Start a ticket only when all its blockers are `done`; stop if there is no executable ticket while unfinished tickets remain.

For every implementation, review, fix, or completion child, use `subagent_start` with the worktree's absolute `cwd`, a short self-contained `prompt`, and a role/ticket `label`. The extension launches a fresh background `pi --approve --mode json --no-session` process; `--approve` trusts this project's resources. Child processes cannot use this extension to delegate recursively. Run the two review children in parallel as described in section 4.

Do not pass the parent transcript, previous child conversations, or implementation dumps. A new process with only a short task prompt is the context reset; compaction or an instruction to forget is not a substitute.

### Child monitoring and deadlines

Announce each returned child ID, log directory, and deadline. The extension creates private temporary logs outside the worktrees, verifies the isolated process group, continuously drains stdout/stderr, parses complete LF-terminated events, and retains exit evidence. `job.json`, `events.jsonl`, `stderr.log`, and `exit-status` remain available for inspection.

- Call `subagent_wait` repeatedly with all active child IDs. Each call waits up to **40 seconds**, returning control to the parent with compact snapshots: active tools and durations, last completed tool, event silence, failure/retry/compaction counts, and exit evidence. No extra sleep or JSONL parser is needed. Aborting a wait does not cancel children.
- Give the user a concise ticket-progress summary at least every **60 seconds** while children run and whenever a ticket completes or its stage changes. Record each ticket's start time when work begins; elapsed time covers the whole ticket, including reviews, fixes, verification, and committing, and never resets at stage or child boundaries. Display durations as `MM:SS` (or `HH:MM:SS` after an hour). Mark completion and freeze its duration only after verification and a successful commit. Do not invent durations for tickets completed before this run.
- Use this format, listing tickets completed in this run, the active ticket's stage and total elapsed time, and the count of unfinished tickets excluding the active ticket:

  ```text
  Ticket 01 completed in 06:12
  Ticket 02 completed in 08:45
  Ticket 03 — verification — 10:32 elapsed
  4 tickets remaining after this one
  ```

  Use stages such as implementation, verification, review (Spec: running/passed; Rules: running/passed), fixes, and committing. Derive stages from workflow state and explicit child progress notes, not guesses about shell activity. After the last ticket, report final spec review/verification separately; ticket completion does not imply spec completion.
- Keep routine summaries free of last-tool details, event-silence counters, and incidental tool-error counts. Continue inspecting that metadata internally; report it only for actionable errors, suspected stalls, deadlines, or blockers. Never expose raw arguments, results, thinking, or potentially sensitive output. JSON events provide activity evidence, not proof of meaningful progress.
- After **five minutes without events**, warn and inspect the owned process tree, stderr, and the last active command. A quiet build/test or provider wait is not automatically a hang. Report what is known; do not invent percentage completion. Continue monitoring a legitimate wait within its deadline. If investigation cannot establish useful activity, stop the child and report a suspected stall.
- Detect repeated identical failures or repair loops even when events keep arriving. Stop on repeated attempts with no meaningful progress rather than treating log growth as success.
- Enforce a **60-minute wall-clock limit per child** with OS-level `timeout`, not merely the harness tool's timeout. Required verification commands must also have finite timeouts within that budget. Do not silently extend deadlines or automatically restart a timed-out child. A different budget requires explicit user agreement before launch.
- On cancellation, suspected stall, or workflow failure, use `subagent_cancel` with the owned IDs. Include both reviewers when abandoning a parallel review cycle. The extension sends TERM, allows at most **10 seconds**, then sends KILL, and checks group cleanup, including after natural exit or deadline. If cleanup cannot be confirmed, stop before any subsequent mutation or review. Never use broad `pkill pi` or kill unrelated processes. Keep logs and worktree changes for inspection; leave unfinished tickets unfinished. Session shutdown/reload cancels children; handles do not survive restart, so do not reload during implementation.
- On exit, read `subagent_wait`'s exit status, final assistant summary, and ticket evidence. `exited` means only that the process finished cleanly with a summary and settled event, not that its task passed; `blocked` requires investigation. Truncated summaries point to the full event log. `agent_end` may be followed by retries; the extension requires process exit and `agent_settled` before reporting a clean exit. Missing exit evidence, malformed/truncated output, timeout (normally status `124`), forced termination, or incomplete reports are blockers, not passes. Report elapsed time, log paths, last activity, and the blocker; do not blindly retry.

For every child prompt, also request brief progress notes at phase boundaries (reading, implementation/fixes, verification, blocked/completed), and finite verification-command timeouts. These notes improve meaning but do not replace parent-side monitoring or deadlines.

For each ticket, construct the implementation prompt with the spec path, ticket path, base commit, and these instructions:

1. Read `AGENTS.md`, the ticket and its source spec, and applicable project documentation and rules. Read `.pi/skills/implement/SKILL.md` for workflow authorization only; do not recursively orchestrate or launch children. Reconstruct needed context from repository files.
2. Mark the ticket `in-progress`; implement only its requirements and necessary tests. Preserve earlier tickets' work. Run its required verification with the documented toolchain.
3. Leave the ticket `in-progress` for independent review. Record concise verification evidence, but do not mark it `done` or perform either review yourself.
4. Do not commit, stage, push, merge, or alter source requirements. Return a brief summary: ticket status, changed paths, checks, and blockers.

Monitor implementation until it exits, then run the independent review cycle in section 4. After both axes pass, launch a fresh completion child to rerun required verification, check the ticket's acceptance criteria, record evidence, mark it `done`, and update newly unblocked tickets according to the lifecycle rules. If verification requires code changes, leave it `in-progress` and return to the review cycle after those changes.

After the parent verifies the completion child's evidence, and with no children active, commit the ticket before advancing:

- Inspect Git status and diffs, including untracked files. Stage only the ticket's implementation, tests, documentation, review fixes, and lifecycle updates; exclude unrelated changes and secrets. Stop if the changes cannot be safely separated.
- Inspect the staged diff, then create one Conventional Commit using the commit skill's message rules and naming the ticket path in the body. Do not amend earlier commits.
- Verify success and that no ticket-owned changes remain uncommitted; report the hash and ticket. Stop on failure.

Only the parent may stage and commit. Every child must read project instructions, preserve unrelated work, and avoid launching children or committing, staging, pushing, merging, or changing requirements. The parent reads each child's exit status, summary, and ticket evidence before advancing. A zero exit status alone is not proof of completion. Stop on process failure, incomplete verification or review coverage, ambiguity, unresolved findings that cannot be safely fixed, or an unfinished ticket; report the worktree and blocker. Do not blindly retry or continue with dependents.

## 4. Independent two-axis review cycle

Once implementation or fixes have exited, launch **two fresh Pi processes in parallel** against the same unchanged worktree. Monitor both using section 3's polling, heartbeat, and deadline policy; capture their exit statuses and reports separately. Do not run any implementation, fix, completion, or mutating verification process while reviewers are active.

Supply each reviewer only the spec path, ticket path (or complete ticket-set path for final review), base commit, scope, and its own brief. Do not pass implementation summaries, conversations, the other reviewer's report, or prior review findings. Reviewers reconstruct context from repository files. Use an explicit, preauthorized scope containing `git diff "$base"` paths, staged/unstaged paths, and non-ignored untracked files. This includes earlier committed tickets and current edits; never rely on the review skills' default edited-file scope.

- **Spec reviewer:** Load `.pi/skills/review-implementation/SKILL.md`. For a ticket, review the explicit accumulated scope against that ticket and its source spec; distinguish earlier tickets' required work from scope creep. For final review, resolve and announce the complete spec implementation scope, including reused existing code and all accumulated edits, and check cross-ticket integration and every spec requirement. Report missing, partial, incorrect, or unrequested behavior with requirement citations and evidence.
- **Rules reviewer:** Load `.pi/skills/review-rules/SKILL.md`. Review all files in the explicit accumulated scope against applicable project rules, including earlier committed tickets. Report violations with rule citations and evidence. Do not perform the spec review.

Both briefs must explicitly say: scope is preauthorized, **report only**, do not apply the review skill's immediate-fix exception, do not edit files or ticket statuses, do not run mutating commands or tests, and do not launch children. Include each skill's required file checklist, coverage limits, actionable findings, and a clear pass or blocker. Verification runs serially outside review; reviewers may inspect existing test evidence and test code but must not treat passing tests as proof of correctness.

The parent reports findings under separate **Spec** and **Rules** headings, without merging or reranking the axes. If either report is incomplete or blocked, stop rather than treating it as a pass. If either has actionable findings, launch a fresh fix child after both reviewers exit, passing both labelled reports plus requirement paths and scope. Instruct it to apply corrections and necessary regression tests, preserve unrelated work, rerun relevant verification, and recheck behavior affected by rules fixes. Never rewrite requirements to fit code. Keep affected tickets unfinished until criteria and reviews pass; reconcile their statuses and evidence.

After fixes, repeat **both reviews in new processes**, with no prior reports in their prompts. Only finish the cycle when both axes independently pass on the latest implementation. Stop if fixes are unsafe, ambiguous, fail verification, or make no progress; report the blocker rather than looping indefinitely.

## 5. Final spec reviews

After every ticket is verified `done`, run section 4 again with the complete spec and ticket set. The final Spec and Rules reviewers must be separate fresh processes, not a shared final-review context.

When both final axes pass, launch a fresh completion child to run required spec-level checks and reconcile all ticket criteria, statuses, and verification evidence. Do not leave `done` claims for unmet criteria. If code or tests change, repeat both final reviews before claiming completion. Commit remaining spec-level fixes and completion evidence using the same checkpoint procedure; skip the commit if there are no changes.

Report completion only when all tickets remain verified `done`, both final reviews are complete, and required checks pass with no unresolved findings. Otherwise report partial completion and blockers. End with separate Spec and Rules results, ticket commit hashes and any final checkpoint hash, the branch, worktree path, and concise verification summary; leave the worktree available for inspection. Do not claim completion while implementation or completion evidence remains uncommitted.
