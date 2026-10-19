# Save files to a provider

Status: Implemented

Source idea: [Save files to a provider](../ideas/03-save-files-to-provider.md)

## Problem Statement

Managed files can be materialized from providers, but local edits cannot be explicitly saved back through psst. Users need to preserve those edits in the configured source, including project-local encrypted storage, without importing arbitrary files or relying on automatic write-back.

## Solution

Provide CLI-only push of all configured managed files to their existing source targets, alongside clearly named pull. Plan exact-text updates in bounded memory, obtain one batch approval, and save independent valid targets with accurate per-file outcomes. Preserve unrelated source data and never modify local inputs.

## User Stories

1. As a managed-file user, I want to explicitly push local edits to their configured sources, so that I can preserve changes without manually re-entering contents.
2. As a CLI user, I want clear transfer directions and one informed approval, so that I can control overwrites interactively or unattended.
3. As a user saving several files, I want independent valid saves and truthful individual results, so that I can recover from partial failures without assuming rollback.

## Implementation Decisions

### Agreed CLI and consent contract

- Canonical commands are `psst --files-pull [--yes]` for provider → local and `psst --files-push [--yes]` for local → provider. Do not retain the former `--write-files` alias; v2 is alpha and requires no legacy pull compatibility. Help and usage must make direction explicit. Reject mutually incompatible operations, including push with pull, copy, environment export, or wrapped-command execution; preserve wrapped arguments after command dispatch or `--`.
- Push selects all configured managed files, never environment entries. An empty set succeeds with guidance.
- Before any source changes, show one batch confirmation, defaulting to no, identifying affected paths, providers, and shared-reference warnings without contents. `--yes` supplies unattended approval, not a safety bypass. Refusal or unavailable consent performs no source writes and returns nonzero.
- Omit unchanged targets from confirmation and persistence. If nothing needs updating, do not prompt; missing or unsupported targets still count as failures.

### Agreed local-input and resource contract

- Interpret paths literally beside the selected configuration; absolute and outside-project paths remain allowed. Refuse symlinked files or ancestors, nonregular files, and configuration/storage artifacts. Missing or unreadable files fail individually. Do not change input contents or permissions, and do not require input mode 0600.
- Preserve exact UTF-8, including empty text, Unicode, whitespace, and newlines. Reject invalid UTF-8/binary input, NUL, and files over 4 MiB (4,194,304 bytes). Enforce stricter provider limits, including local storage's 1 MiB (1,048,576 bytes), and existing persisted-format bounds.
- Retain planned local-file plaintext only in memory in zeroizing buffers. Cap aggregate planned local-file plaintext at 64 MiB (67,108,864 bytes), counted before deduplication. Exceeding this cap aborts the batch before writes; no plaintext disk fallback or staging is allowed. Bound provider working buffers as well. These snapshot restrictions do not prohibit the intended persistence of manual literals in configuration JSON.
- Immediately before persistence, revalidate every participating local input's identity, path safety, and exact contents against planning. If an input changed, fail its target instead of saving stale or newly read contents. Never modify the input to resolve a conflict.
- Never put secret contents in diagnostics or argv. Cancellation discards owned buffers and unfinished work; completed writes remain. Interrupted remote writes must be reported as potentially uncertain, not assumed undone.

### Agreed target resolution and grouping

- Resolve source targets before grouping; do not equate references solely by their textual spelling. Identical contents for one resolved target coalesce into one update. Differing contents fail that entire target, never last-writer-wins; unrelated targets continue.
- Compare against exact raw stored values: manual literals, decrypted local-storage records, and raw `op` field values, not normalized fetch output. Exact matches are unchanged.
- Approval authorizes overwriting the latest value of the intended existing target, even if its value changed after planning. This is not stale-source conflict rejection. Reload latest source state, verify that the mapping still identifies the approved target, and overwrite only approved targets while preserving unrelated data. Removed, retargeted, or newly unsafe targets fail.
- Retain existing locks, integrity checks, and loaded-content conflict checks between the latest reload and persistence. A final local race may fail rather than clobber a competing write. `op` retains its remote race; a reread alone does not establish conditional-write safety.

### Agreed writable providers

- **`local-storage`:** update only an existing record selected under existing reference rules by one unambiguous matching eligible SSH-agent key. Qualified references restrict the key; unqualified references require exactly one matching key/record. Missing records or keys, ambiguity, agent failures, and integrity failures fail the affected target. Preserve unrelated IDs and other keys' storage, existing cryptography, private storage permissions, locks, and atomic replacement. Do not create records or offer alternate-key selection. Storage remains directly under `.psst/` beside the selected configuration.
- **`manual`:** update the configured managed file's literal value in JSON. Commit all valid approved manual-file updates together in one atomic configuration save against latest state, preserving environment entries and unrelated mappings. Configuration-save failure affects this group independently of completed provider writes; distinguish failure before replacement from uncertain durability after replacement.
- **`op`:** update existing text/concealed fields only in items whose safe round trip can be established. Resolve an unambiguous existing vault/item/field target; safely combine approved different-field updates within one item, preserving other fields and metadata from latest state. Reject passkey-bearing items, attachments, derived OTP targets, unsupported or ambiguous targets, and item/field creation. The reviewed CLI documentation also excludes editing SSH keys. Fail closed when safe item eligibility cannot be established; general CLI editing support is not proof that an arbitrary item is safe. Send secret-bearing payloads over stdin, never argv, with bounded, cancellable subprocess work and content-free errors.
- Unsupported providers, including Vault, Terraform, shell, derived `op 2FA`, and unknown sources, fail their managed files without preventing unrelated valid saves. Do not execute shell commands to infer a reverse operation.

### Agreed results and compatibility

- Saving is best effort across independent targets and persistence groups. Any failure makes the overall status nonzero; retain completed changes with no cross-provider rollback.
- Report every configured managed file as saved, unchanged, or failed, including all files represented by a coalesced target or grouped persistence operation. Explicitly identify partial completion, uncertain durability, and uncertain remote outcomes. Refusal and cancellation are also nonzero; do not report uncertain changes as definitely saved or rolled back.
- Pull receives naming/alias changes only: preserve its existing content, consent, safety, and failure semantics. Document that `op` fetch trims output and some providers reject empty reads; exact push does not promise byte-exact push→pull round trips. Other environment, copy, export, and TUI behavior remains unchanged.

## Out of Scope

- Arbitrary-file import, single-file/subset selection, automatic write-back, TUI saving, dry-run, JSON output, and watch modes.
- A new plaintext-file provider; local-file source updates mean existing encrypted local storage.
- Writes to Vault, Terraform, shell, or derived OTP; creation of local-storage records or `op` items/fields; alternate-key selection; unrestricted `op` item rewriting.
- External backup, guaranteed survival of whole-project deletion, cross-provider transactions/rollback, guaranteed remote race exclusion, and changes to pull normalization or empty-read semantics.

## Further Notes

- Project-local encrypted preservation is sufficient; users may commit `.psst` ciphertext to Git. Keeping the SSH key alone cannot recover deleted ciphertext. Existing crypto, key-custody, history/replay, zeroization, and recovery limitations continue to apply as recorded in [spec 01](01-local-storage.md#further-notes).
- Internal modules/types and the detailed safe-`op` eligibility mechanism remain implementation-owned within the fail-closed restrictions above. Shared-reference warnings cannot establish complete discovery across other configurations.
- The source idea records a pre-existing documentation discrepancy: spec 02 uses `psst write-files`, whereas the verified implementation uses `psst --write-files`. This spec defines the new canonical flags without retaining that former alias; it does not revise that historical document or claim its absent verification ticket exists.
- The source idea retains superseded naming and stale-source-rejection proposals; its final agreement, synthesized here, governs this feature.
