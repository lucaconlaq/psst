# 01: Preserve mixed configurations and isolate environment operations

**Source:** [Managed files](../../specs/02-managed-files.md)

**What to build:** Load and preserve environment entries and managed files together while existing environment operations continue to act only on entries.

**Blocked by:** None (can start immediately)

**Status:** done

Final accumulated verification: all seven criteria remain satisfied. Supplied independent static Spec PASS 30/30 and Rules PASS for introduced changes 30/30 against fixed base `20f516288522578052ef853a795a5dcb340fca0c`, followed by a fresh unchanged-source Linux Rust 1.88.0 configured-check run (266 tests passed), reaffirm this ticket. See [ticket 07](07-release-verification.md#final-local-reconciliation) for commands, immutable snapshot identity, reviews, and the outstanding native macOS release gate. Earlier pending statuses and dependent-ticket transitions below are historical, not current; no full-spec/release completion is claimed.

- [x] Retain insertion-ordered flat JSON with string source/value fields. Missing type loads as env; saves emit explicit env/file tags. Reject malformed or unknown tags and duplicate keys, including cross-kind collisions.
- [x] Use the destination key as the managed file's sole identity. Preserve existing environment-name validation; reject empty destinations and control characters without expanding home or environment variables. TOKEN and ./TOKEN can coexist without overwriting either mapping.
- [x] Preserve both kinds through existing entry editing, renaming, and deletion, including unknown-provider mappings remaining editable. Mapping changes do not alter destination files or provider contents.
- [x] Preserve configuration discovery (PSST_CONFIG, psst.json, legacy .psst.json), provider identifiers, configuration size/NUL validation, loaded-content conflicts, locking, atomic persistence, and pre-/postreplacement failure behavior.
- [x] Wrapped commands and --write exclude file mappings from resolution, injection, and export and never materialize them. Existing environment failure semantics remain, including local-storage failures stopping launch/export.
- [x] Verify legacy and mixed round trips, invalid schemas/paths/names, collisions, discovery, persistence conflicts/faults, and content-free diagnostics. Executable tests use a file-provider side-effect marker to prove environment operations never fetch files.
- [x] Keep the full configuration independent of UI selection; do not adopt the POC's active/inactive map swapping. Internal representation remains implementation discretion, with regression coverage for any necessary prefactor.

## Implementation verification

Completed after independent Spec and Rules re-reviews and the final Linux verification below. Earlier in-progress notes are retained as history.

- Linux, pinned `mise exec -- rustc --version`: Rust 1.88.0. GCC supplied with `nix-shell -p gcc` because the host PATH lacks `cc`; no toolchain or dependency changes.
- Resume verification passed `mise exec -- cargo fmt` and `mise exec -- cargo fmt --check` (60-second limits each), and `mise exec -- cargo clippy --locked --all-targets -- -D warnings` (90-second limit), inside `nix-shell -p gcc` with a 180-second outer limit. Output: `/tmp/psst-ticket01-resume/format-clippy.log`.
- Fixed the existing PTY clipboard fixture's NixOS portability failure by replacing hardcoded `/bin/cat` with PATH-resolved `cat`; exact-byte and helper-failure assertions remain unchanged.
- Full `mise exec -- cargo test --locked` passed inside `nix-shell -p gcc` (210-second test limit, 240-second outer limit): 203 passed, zero failed, ignored, or filtered out. Includes `ctrl_y_copies_exact_bytes_and_reports_failure_in_all_five_views` and the mixed-configuration local-storage failure test. Output: `/tmp/psst-ticket01-resume/tests.log`. These results supersede the earlier failed clipboard run and skipped-test verification recorded in `/tmp/psst-ticket01-verification.log`.
- New coverage: mixed/legacy ordered round trips, explicit tags, malformed schemas and cross-kind duplicates, literal destinations, invalid keys/NUL/size limits, conflicts and all persistence fault boundaries, Entries editing/rename/collision/deletion preservation, discovery variants, file-provider side-effect markers, and fatal environment local-storage failures.
- Verification limits: Linux verification passed; the user explicitly defers macOS verification until all tickets are implemented, so it does not block ticket 01. Independent reviews remain pending; no reviews, staging, commits, push, merge, or child processes were performed during this resume. Ticket remains `in-progress`.
- Review correction (base `20f516288522578052ef853a795a5dcb340fca0c`): addressed Spec P2 by requiring string deserialization for present tags while retaining the absent-tag env default and explicit string serialization. Added rejection/privacy cases for both object-shaped tags (`{"env":null}` and `{"file":null}`). The regression failed before the correction (`invalid config accepted`); afterward all four mixed-configuration tests passed. Supplied Rules review passed 15/15 with no findings; fresh independent re-review of this correction remains pending.
- Correction verification passed on Rust 1.88.0 with GCC via `nix-shell -p gcc`: `cargo fmt`, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, targeted `cargo test --locked --test mixed_config`, and full `cargo test --locked` (203 passed, zero failed/ignored/filtered). All Cargo commands used `mise exec --`, with respective 60/60/90/60/210-second limits and a 480-second outer limit. Logs: `/tmp/psst-ticket01-review-fix/regression-before.log` and `/tmp/psst-ticket01-review-fix/verification.log`. Ticket remains `in-progress`; macOS remains deferred by the user. No requirements edits, child processes, staging, commits, push, or merge.

### Completion verification

- Independent re-reviews against base `20f516288522578052ef853a795a5dcb340fca0c`: Spec **PASS**, 15/15 files, no findings or blockers (`/tmp/psst-subagent-ZxMOuY/events.jsonl`); Rules **PASS**, 15/15 files, no violations or blockers (`/tmp/psst-subagent-c6AxKo/events.jsonl`). Both reviewer processes exited successfully. These supersede the pending-review notes above; no self-review was substituted.
- Final Linux rerun: Rust `1.88.0 (6b00bc388 2025-06-23)`, GCC via `nix-shell -p gcc`. Passed `mise exec -- cargo fmt`, `mise exec -- cargo fmt --check`, `mise exec -- cargo clippy --locked --all-targets -- -D warnings`, and `mise exec -- cargo test --locked`. Limits: 30 seconds for toolchain identification, 60/60/90/210 seconds for the checks, 480 seconds for the outer shell. Full suite: **203 passed**, zero failed, ignored, or filtered out. Evidence: `/tmp/psst-managed-files-01-final/verification.log`.
- Acceptance evidence, checked against the independent Spec trace and final test results:
  1. Ordered legacy/mixed round trips and explicit tags; malformed/string-field/tag schemas and duplicate/cross-kind keys: all four mixed-configuration tests passed, including object-shaped-tag rejection.
  2. Destination-key identity, literal paths, `TOKEN`/`./TOKEN` coexistence, invalid environment names, empty/control-character destinations: mixed round-trip, schema-validation, and save-validation tests passed.
  3. Unknown-provider mappings, entry editing/rename/collision/deletion, and unchanged destination/provider bytes: `entry_edits_rename_and_deletion_keep_hidden_file_mappings` passed.
  4. Discovery and provider compatibility, size/NUL bounds, conflicts, locks, atomic persistence, and pre-/postreplacement faults: mixed size/conflict tests, configuration unit tests (including `mixed_config_failures_track_replacement_and_preserve_both_kinds`), provider suites, and discovery fixtures all passed.
  5. No file resolution/injection/export/materialization by environment operations: executable `environment_operations_never_fetch_export_or_materialize_files` passed with provider side-effect markers and discovery variants. `mixed_configuration_keeps_local_storage_environment_failures_fatal` passed for launch/export failure semantics.
  6. Required regression cases and content-free diagnostics: mixed schema/privacy, round-trip, validation, collision, persistence fault, and executable isolation tests passed as traced above.
  7. Full configuration remains independent of UI selection, with no active/inactive map swapping: independently confirmed by Spec review; the hidden-file-mapping UI preservation regression passed.
- `git diff --check` passed. Tracked code/test/documentation diff outside tickets was byte-identical before and after verification, and the untracked mixed-configuration test checksum was unchanged; comparison artifacts are in `/tmp/psst-managed-files-01-final/`. No code/test changes or new reviews were needed.
- macOS verification remains explicitly deferred by the user until the final release gate, not waived or claimed complete. No ticket-01 blockers remain. Tickets 02 and 04 are now `ready-for-agent`, retaining their blocker references to this completed ticket. This completion pass spawned no child agents, performed no self-reviews or requirements edits, and did not stage, commit, push, or merge.
