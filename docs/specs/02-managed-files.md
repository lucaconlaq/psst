# Managed files

Source idea: [Managed files](../ideas/02-managed-files.md)

## Problem Statement

Projects need local configuration files as well as environment variables. Users
currently have to retrieve and place those files themselves, separately from the
provider references and interactive workflow they already maintain in psst.

## Solution

Manage independent destination-to-provider mappings alongside environment entries.
Author them in a distinct Files dashboard view and materialize them explicitly
with `psst write-files`. Preserve the existing provider text semantics, require
batch consent for replacements, and write files privately with per-file failure
reporting. The approved POC supplies the navigation design, not production safety
or implementation architecture.

## User Stories

1. As a developer, I want to configure file destinations and provider sources
   alongside environment entries, so that one configuration describes my local
   environment without treating file contents as environment variables.
2. As a developer, I want to create, browse, edit, and remove managed-file mappings
   interactively, so that I can use familiar provider controls without manually
   editing JSON or unintentionally changing destination files.
3. As a developer or automation author, I want to explicitly materialize my
   configured files with clear consent, safety checks, and outcomes, so that I can
   prepare my environment and recover from partial failures without guessing what
   changed.

## Implementation Decisions

### Verified current baseline

These are existing capabilities, not evidence that managed files are implemented:

- Configuration is an insertion-ordered flat JSON object of environment names
  mapped to string `source` and `value` fields. Loading validates names, rejects
  duplicate keys and NUL, and bounds configuration size. Saving uses cooperative
  locking, loaded-content conflict detection, and atomic replacement.
- Configuration discovery supports `PSST_CONFIG`, `psst.json`, and legacy
  `.psst.json`; provider working directories derive from the chosen configuration.
- The provider registry owns fetching, editor metadata, optional reference
  browsing, and optional local-storage authoring. Shared fetching returns
  zeroizing UTF-8 strings and rejects NUL. Unknown providers remain editable but
  cannot resolve.
- The process boundary bounds captured output, enforces existing timeouts, and
  trims subprocess text. Provider-specific empty-value behavior differs. Bulk
  environment resolution is concurrent, whereas this feature requires sequential
  file resolution.
- The UI has shared editor/browser controls, on-demand reveal/copy, cancellation
  and stale-result handling. Atomic persistence exposes failures before and after
  replacement, including directory-sync failures.

### Configuration contract

- Retain the flat JSON object. Each mapping contains string `source` and `value`
  fields and a `type` tag: `env` for an environment entry or `file` for a managed
  file. Missing `type` loads as `env`; saving emits explicit tags for both kinds.
  Reject unknown tags and duplicate object keys, including collisions between
  kinds. Preserve existing configuration discovery, validation applicable to
  environment entries, persistence conflict handling, and provider identifiers.
- A managed file's destination is its object key and sole identity, not a separate
  logical name or an environment-variable association. Both kinds can reference
  the same provider contents independently.
- Distinct keys `TOKEN` (environment entry) and `./TOKEN` (file) can coexist.
  A cross-kind key collision must not overwrite the existing mapping.
- Destination paths are literal: no home or environment-variable expansion.
  Reject empty paths and control characters. Relative paths resolve beside the
  defining configuration, not beside the caller's working directory. Absolute
  paths and paths outside that directory are allowed subject to the safety
  contract below.
- File mappings are excluded from wrapped-command environment injection and
  `--write` dotenv export. Neither action materializes files. Existing environment
  failure semantics, including local-storage failures stopping those operations,
  remain unchanged.
- Saving, renaming, or deleting a file mapping does not write or remove destination
  files. Deletion also keeps provider contents.

### Interactive contract

- Start on Entries. Dashboard Tab switches Entries/Files while preserving each
  view's filter and selection. Tab in editors, browsers, and dialogs retains its
  existing contextual behavior. Switching views must not leak revealed values or
  accept pending results from the previous view.
- Use Ratatui's actual `Tabs` widget on the list's top border, showing Entries and
  Files together with padded labels, a dot divider, and a contrasting filled
  active tab. This follows the approved POC and the
  [Ratatui Tabs example](https://ratatui.rs/examples/widgets/tabs/). Do not place
  navigation beside the branding as plain labels or badges: both were too easy
  to miss during exploration. Advertise Tab switching in the footer.
- Put the concise count/filter title on the bottom border: for example,
  `Files · 1`. Materialization instructions belong in contextual status/help, not
  in that title. Preserve usable empty and filtered states.
- Files shows Destination, Source, Reference, and Contents columns and a
  selectable `+ New file` row. Ctrl+N or Enter on that row opens a draft; Enter on
  an existing row edits it. Use the existing type-to-filter, selection, help,
  discard, and quit conventions.
- Reuse the provider editor, source selector, reference browser, and local-storage
  authoring flow. Label the first file-editor field Destination rather than
  Environment name. Ctrl+B browses where supported; Ctrl+S saves the mapping,
  not its destination. Explain the separate `psst write-files` step after saving.
  Local-storage authoring remains separate from linking a mapping.
- Contents stay masked by default. Reveal/copy uses the existing on-demand
  provider-result behavior, including draft-only manual editing, short-lived
  revelation, and cancellation. Never fetch automatically for the dashboard or
  read destination contents for reveal/copy. Shell references execute only when
  their resolution is requested.
- Ctrl+D confirms mapping deletion without a ciphertext-removal toggle for file
  mappings. Existing provider-browser record actions remain separate. Shared
  reference warnings for provider authoring/deletion must count environment
  entries and managed files in the loaded configuration; they do not establish
  usage in other configurations.

### Materialization contract

`psst write-files [--yes]` targets all configured managed files. Its phases are
ordered so that partial success never bypasses batch replacement consent.

1. **Validate configuration-wide destinations.** Before provider fetching, reject
   aliases that resolve to the same destination and destinations targeting psst
   configuration files, their lock files, or `.psst` storage. These are
   configuration-wide errors: stop the run rather than process a subset.
2. **Resolve and stage.** Resolve files sequentially through existing providers,
   preserving their working-directory, timeout, stricter size, and empty-value
   rules. Limit each resolved UTF-8 result to 4 MiB. Stage successful results in
   0600 temporary files inside a private 0700 system-temp directory outside the
   destinations. Keep planning metadata bounded; do not retain every resolved
   file in memory. Write the provider result without added trimming, newline,
   interpolation, or other transformation. Existing provider normalization still
   applies; a successful empty result materializes an empty file.
3. **Plan and obtain consent.** Report individual planning/resolution failures
   before confirmation. Identify valid differing existing files and list all of
   them in one confirmation before any destination writes, including new files.
   If none require replacement, no replacement approval is needed. `--yes`
   approves replacements without prompting; otherwise noninteractive replacement
   fails before writing anything. Interactive refusal cancels the entire run.
   The POC's `Replace all? [y/N]` is the demonstrated prompt, with refusal as the
   default; contents must never appear in it.
4. **Persist independently.** After consent, process valid planned files under the
   filesystem contract below. A per-file resolution, planning, or write failure
   does not prevent independent valid files from succeeding. This best-effort
   behavior includes local-storage resolution failures in this command; it does
   not change the environment-operation rules above.
5. **Report and clean up.** Report destination paths with `written`, `unchanged`,
   or `failed` outcomes, never resolved contents. Any error or declined
   confirmation makes the command unsuccessful. Empty configurations/file sets
   and no-change runs succeed; the empty-file case should guide users to Files.
   Clean staging and unfinished temporary files on normal exit, errors, and
   cancellation. Ctrl+C exits unsuccessfully, cleans temporary files, and keeps
   already completed destination writes.

### Filesystem and persistence contract

- Refuse symlinked destinations or ancestors and nonregular destination targets.
  Newly created parent directories are 0700; existing directory modes are left
  unchanged. Destination files are private 0600 files.
- Only an existing regular file already at 0600 with identical resolved contents
  qualifies as an unchanged no-op. Identical contents with incorrect permissions
  produce a permissions error, not silent permission repair. `--yes` never
  bypasses safety checks.
- Use private same-directory temporary files, syncing, and atomic replacement
  for each destination. Immediately before writing, recheck path safety and
  destination conflicts against the plan; fail a changed destination instead of
  overwriting it under stale consent.
- Replacements are current-user private files. Do not preserve previous modes,
  ownership, ACLs, or extended attributes.
- There is no all-files transaction or rollback. A failure for one file retains
  completed writes to others. Distinguish failures before replacement from
  failures after replacement, such as uncertain durability following a sync
  error; reporting must not claim that an already replaced file was unchanged.

### Responsibilities and bounded discretion

Configuration owns mapping kinds, compatibility, and persistence. Providers keep
ownership of source interpretation and browsing/authoring; materialization owns
planning, consent, staging, path validation, persistence, and outcomes. The UI
edits configuration and requests provider operations, not filesystem writes.

Internal types, helper/module boundaries, alias detection, conflict snapshots,
and staging ownership/cleanup mechanisms remain implementation choices constrained
by the contracts above. Do not adopt the POC's active/inactive configuration-map
swapping as a settled architecture. Likewise, `./config.local` and `REPLACE_ME`
were disposable draft shortcuts, not approved production defaults.

## Testing Decisions

### Seams

The following is a proposed verification plan, not a claim of completed tests or
an additional product decision:

- Prefer the existing executable-level CLI/PTY seam with isolated configurations,
  temporary directories, fake provider executables, and fake SSH agents. It can
  verify most configuration-to-provider-to-filesystem behavior, interactive
  consent, terminal interaction, exit status, and cancellation without real
  authentication or personal secrets.
- Retain UI state and Ratatui TestBackend tests for tab presentation, small-terminal
  layout, filtering/selection, masking, and stale results that are awkward to
  establish through PTY output alone.
- Reuse configuration round-trip and persistence fault-injection patterns for
  deterministic lower-level evidence. Propose a narrow materialization test seam
  at plan/commit boundaries to inject destination changes and staging/write/sync
  failures. The seam should expose resulting filesystem state and outcomes, not
  mandate a particular internal implementation.

### Acceptance coverage

| Boundary | Required coverage |
| --- | --- |
| Configuration and compatibility | Legacy untagged reads; explicit tag saves; mixed mappings surviving edits in either view; malformed/unknown tags and duplicate keys; invalid names/paths; cross-kind collisions and `TOKEN`/`./TOKEN`; existing discovery, save-conflict, and diagnostic privacy behavior. |
| TUI | Start view, Tab filter/selection restoration, visible attached tabs and concise bottom titles, empty/filter states, creation row, editing/source selection, browser and local-storage authoring, save/discard/deletion without destination writes, provider retention, shared warnings across both kinds, help and terminal restoration at supported sizes. |
| Interactive contents | No automatic fetching or destination reads; requested source resolution versus draft-only editing; reveal expiry, copy, cancellation, and stale-result rejection across tab/editor changes; no inadvertent plaintext diagnostics. |
| Resolution and isolation | Existing provider types through controlled fixtures; configuration-relative and absolute/outside destinations; caller cwd independence; literal path handling; provider normalization, successful empty output, rejected empty output where applicable, NUL/UTF-8 failures, timeouts, exact limit/over-limit results, stricter provider bounds, and sequential fetching. Verify files are not injected/exported or written by wrapped commands. |
| Planning and consent | Configuration-wide invalid destinations stop before a fake provider records any fetch. Mixed new/unchanged/differing/failed plans report failures before one replacement prompt; refusal or missing noninteractive consent leaves every destination untouched. Exercise approval, `--yes`, and runs requiring no prompt. |
| Safety and persistence | Destination aliases, protected artifacts, symlinked ancestors/destinations, nonregular targets, new/existing directory modes, private staging outside destinations, final permissions, unchanged permission errors, and no safety bypass with `--yes`. Mutate targets/ancestors between planning and commit; verify refusal without clobbering and independent successes. Inject temporary-write and sync failures around atomic replacement and verify actual surviving bytes and partial-outcome reporting. |
| Cleanup and outcomes | Successful and empty runs; best-effort failures including local storage; normal/error/refusal cleanup; Ctrl+C during resolution, waiting for consent, and between writes; no abandoned normal-run staging or unfinished destination temporaries, completed writes retained, nonzero failure/cancellation status, and content-free path/outcome reports. |
| Resource behavior | Private on-disk staging and bounded planning rather than aggregate plaintext retention, with many files and near-limit results; no fetching beyond a failed configuration-wide preflight. |

### Release verification

Before considering the implementation complete, run the repository's configured
formatting, Clippy, and full test suite with the pinned toolchain, including the
acceptance coverage above on supported Linux and macOS environments. Review the
path/conflict and cleanup implementation against the agreed contract; passing
ordinary happy-path tests is insufficient. Update user-facing usage/help with the
command, TUI controls, explicit-save/materialize distinction, outcome semantics,
and plaintext/crash limitations.

Real-provider checks, if useful, must be opt-in with disposable destinations and
explicit authentication consent, never a dependency of normal tests. Existing
local-storage release limitations remain in force; this feature does not grant
new production approval to that provider.

## Out of Scope

- Reverse synchronization or uploading locally edited destination contents to
  providers; source-write conflict workflows.
- Templating, interpolation, binary attachments, and dedicated file downloads.
- Automatic materialization before wrapped commands or from the TUI; destination
  cleanup when mappings are deleted or renamed.
- File subset selection, local-content viewing, comparison UI, JSON output,
  dry-run, and watch modes.
- Startup sweeping, automatic crash recovery, and preservation of destination
  metadata beyond the specified replacement contract.
- A separate application/project, new provider types, or treating prototype
  placeholders and shortcuts as final product behavior.

## Further Notes

Managed destinations and staging contain plaintext. A crash can leave plaintext
staging behind despite normal cleanup; users must be warned and may need to
inspect and remove those leftovers manually. Paths outside the configuration
root are deliberately allowed, not sandboxed. Trusted shell-provider resolution
executes commands. Batch refusal prevents psst destination writes, not side
effects a provider command may already have performed.

The POC was discarded after the user approved its final presentation. Its initial
full suite and later UI/Clippy checks are historical interface evidence only;
the safety, limits, staging, and cancellation coverage in this spec still require
implementation and verification. This document creates no tickets and does not
authorize implementation.
