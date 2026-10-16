# 05: Materialize files with batch consent and safe persistence

**Source:** [Managed files](../../specs/02-managed-files.md)

**What to build:** Run psst write-files [--yes] against all managed files, obtaining batch replacement consent before safely persisting independent results.

**Blocked by:** 04 — Validate destinations and stage a bounded materialization plan

**Status:** blocked

- [ ] Dispatch the explicit command using existing configuration discovery, without changing ordinary wrapped-command argument forwarding or materializing from the TUI. Do not add subsets, dry-run, watch, JSON output, or automatic materialization.
- [ ] Report planning/resolution failures before one prompt listing every valid differing existing file, without contents. Use Replace all? [y/N] with refusal default. No destination writes, including new files, occur before consent.
- [ ] --yes approves replacements only, never bypasses safety. Without it, noninteractive replacement fails before all writes; interactive refusal cancels the entire run unsuccessfully. Runs without replacements need no approval.
- [ ] After consent, persist valid files independently. Create missing parents at 0700 while preserving existing directory modes; create current-user private 0600 destinations using private same-directory temporary files, syncing, and atomic replacement.
- [ ] Immediately before writing, recheck ancestor/destination safety and conflicts against the plan. Fail changed destinations instead of using stale consent, while allowing unrelated valid files to succeed.
- [ ] Do not preserve previous modes, ownership, ACLs, or extended attributes. Retain unchanged no-ops and permission errors from planning; never silently repair an identical nonprivate file.
- [ ] Report destination paths with written, unchanged, or failed outcomes, never contents. Any error makes the command unsuccessful, including local-storage resolution failures, while independent valid files can succeed. Empty sets succeed with guidance to Files; no-change runs succeed.
- [ ] Distinguish pre-replacement failures from replacement completed with uncertain durability. Never claim an already replaced file was unchanged or rolled back. There is no batch transaction; completed writes survive other failures.
- [ ] Clean staging and unfinished destination temporary files on normal exit, error, and refusal. Structure ownership so cancellation can clean unfinished work without undoing completed writes; exhaustive signal/fault verification belongs to ticket 06.
- [ ] Verify executable and PTY mixed new/unchanged/differing/failed batches, approval/refusal/default refusal, --yes, missing noninteractive consent, empty/no-change runs, private parent/final modes, unchanged existing directory modes, no safety bypass, partial outcomes, and content-free reports. Verify CLI help exposes the command.
