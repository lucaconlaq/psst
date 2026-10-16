# Managed files

Status: Agreed

## Executive summary

Allow psst to manage files alongside secrets. These files are usually untracked, environment-related configuration files retrieved from a specified source and written to a specified filesystem destination. A command such as `psst write-files` would materialize them.

## Notes

Original capture (historical context; decisions below supersede resolved uncertainties):

- Specify a source and destination for each managed file, analogous to specifying a source for a secret.
- Files may be fetched from different sources; the supported sources are not yet defined.
- In the future, support the reverse operation: update a source from a locally edited file where the source supports writes. Some sources are read-only.
- The first-iteration scope remains unspecified: the original description ended at “in our first iteration”.
- Terminology is unresolved: the current glossary defines providers, entries, references, and configuration in terms of secrets and environment variables. How managed files relate to those concepts requires discussion.

## Design tree

- Q1 — Initial direction: one-way materialization only (confirmed). Reverse updates are explicitly excluded from the first iteration, keeping source-write and conflict handling out of scope.
- Q2 — File contents: preserve source contents unchanged (confirmed). Templating and interpolation are explicitly excluded.
- Q3 — Sources (after provider investigation): reuse existing secret providers for resolved text (confirmed). Dedicated file downloads and binary attachments are excluded from v1.
- Q4 — Materialization workflow: explicit `write-files` command only (confirmed). Automatic writes before wrapped commands and TUI materialization actions are excluded.
- Q5 — Domain model (depends on Q3): separate managed-file mappings with their own source and destination, independent of environment-variable entries (confirmed). Broader glossary alignment remains open.
- Q6 — Authoring workflow (depends on Q4): include interactive creation, source selection, editing, and deletion (confirmed). Q4 still restricts materialization to the explicit command.
- Q7/Q9 — Destination paths: relative paths resolve beside the defining `psst.json`; absolute paths and destinations outside that directory are allowed (confirmed).
- Q8/Q10 — Existing destinations: one confirmation lists all differing existing files before any writes. Identical files are no-ops. Declining cancels the entire run. Noninteractive replacement fails unless `--yes` is supplied (confirmed).
- Q11 — Identity: the destination identifies each managed-file mapping; no separate logical name (confirmed).
- Q12 — Vocabulary: broaden Provider, Reference, and Configuration to cover secrets and managed files; Entry remains specific to environment variables (confirmed).
- Q13/Q21 — Configuration: retain a flat object; managed-file mappings use destination keys and `type: "file"`; environment entries use `type: "env"`. Missing type reads as env for backward compatibility, and saving adds explicit env tags (confirmed; supersedes the earlier untagged-save proposal). Q25 confirms `env` spelling, rejects unknown tags and duplicate keys, and permits `./TOKEN` as the distinct file key when environment entry `TOKEN` exists.
- Q14 — TUI: separate Entries and Files views with shared editing and provider-browsing controls; destination replaces environment name in the file editor (confirmed).
- Q15 — Filesystem safety: private 0600 files; create missing parents as 0700, preserve existing directory modes, refuse symlinked destinations or ancestors and nonregular targets (confirmed).
- Q16 — Failure handling: independent best effort; successful files may be written despite other failures, report individual outcomes and fail the command if any file fails (confirmed). Reconcile this with Q10 batch confirmation before any writes; atomic per-file persistence remains to be settled.
- Q17 — Text semantics: write existing provider results without additional transformation; existing whitespace trimming and source-specific empty-value behavior remain (confirmed). Q2 refers to resolved contents, not byte-for-byte source preservation.
- Q18 — Selection: `write-files` targets all configured managed files; subset selection excluded from v1 (confirmed).
- Q19 — Removal: deleting or renaming mappings never removes existing destination files (confirmed).
- Q20 — Limits: 4 MiB resolved text per file, retaining stricter provider bounds and existing timeouts; resolve sequentially with bounded planning metadata (confirmed).
- Q22 — Navigation: Tab switches Entries/Files on the dashboard, preserving each view’s filter and selection; start on Entries; existing Tab behavior elsewhere stays unchanged (confirmed).
- Q23/Q27 — Batch planning: stage successful fetches in 0600 temporary files in a private 0700 system-temp directory outside destinations; report failures before one confirmation for valid replacements. Declining writes no destinations. Clean staging on normal exits/errors/cancellation. Document that crashes can leave plaintext; automatic recovery and startup sweeping are excluded (confirmed).
- Q24 — Persistence: atomic per-file replacement with private same-directory temporary files and syncing; recheck path safety and destination conflicts before writing, fail changed destinations, and retain completed writes when another file fails (confirmed).

- Q26 — Invalid destinations: reject aliases resolving to the same destination and destinations targeting psst configuration/lock files or `.psst` storage before fetching; these are configuration-wide errors (confirmed).
- Q28 — File editing: reuse provider browsing and local-storage authoring. Deleting a managed-file mapping keeps provider contents. Shared-reference warnings count both environment entries and managed files (confirmed).
- Q29 — Identical files: only regular files already at 0600 qualify as unchanged no-ops; otherwise report a permissions error without silently repairing permissions. `--yes` does not bypass safety (confirmed).
- Q30 — Interactive contents: reveal/copy resolves the provider result on demand, never reads the destination; reuse existing masking/reveal/copy behavior. No automatic dashboard fetch, local viewer, comparison UI, or TUI materialization (confirmed).
- Q31 — Outcomes: report paths and written/unchanged/failed outcomes, never contents. Empty configuration/no changes succeeds; any error or declined confirmation fails. Ctrl+C cleans temporary files, retains completed writes, and exits unsuccessfully. JSON output, dry-run, and watch modes are excluded (confirmed).
- Q32 — Paths and metadata: paths are literal, without home/environment expansion; reject empty paths and control characters. Replacements are private current-user files; preserving previous modes, ownership, ACLs, or extended attributes is excluded (confirmed).

## Open questions and investigations

- Q1–Q32 are settled or explicitly exclude their alternative branches. No open design questions or investigations remain; the user confirmed the shared understanding.
- Verified: existing providers return zeroizing UTF-8 strings and shared fetching rejects NUL (`src/providers/mod.rs`). Existing configuration maps environment names to `{source, value}` entries (`src/config.rs`).
- Verified: `src/process.rs` captures at most 4 MiB, requires NUL-free UTF-8, applies `PSST_TIMEOUT` (default 30 seconds), and trims surrounding whitespace. Shell and 1Password reads use it (`src/providers/shell.rs`, `src/providers/op.rs`); 1Password also rejects empty results. Q2 unchanged contents must be clarified against this existing normalization.
- A sub-agent is unavailable in this environment; investigation is being performed directly.
- No same-filename spec or linked tickets were found when reopening.

## Agreement

The user explicitly confirmed the shared understanding and Q1–Q32 decisions. V1 adds independently configured managed files using existing provider results, explicit env/file tags in the flat configuration, separate interactive views, and an explicit all-files `write-files` command. Materialization uses private staging, batch replacement confirmation, safe atomic per-file writes, and best-effort outcomes. Relative paths resolve beside the defining configuration; outside paths are allowed subject to the recorded safety checks. Existing provider normalization remains; crash-leftover plaintext staging is an acknowledged limitation.

Reverse updates, templating, binary attachments, automatic materialization, and automatic destination cleanup are excluded. This agreement authorizes neither specification generation nor implementation; each requires a separate request.
