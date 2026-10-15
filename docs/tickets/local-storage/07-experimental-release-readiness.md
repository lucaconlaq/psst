# 07: Document and verify experimental release readiness

**Source:** [Agreed spec](../../specs/01-local-storage.md), including its linked idea and exclusions.

**What to build:** A documented, verifiably experimental feature with accurate compatibility evidence and explicit outstanding production-readiness gates. Documentation and release verification do not authorize contacting personal agents without consent or advertising production readiness prematurely.

**Blocked by:** 03 — Resolve encrypted references safely across runtime consumers; 05 — Reuse and update encrypted records through the TUI; 06 — Delete mappings with optional, identity-safe ciphertext removal

**Status:** in-progress

- [ ] Document setup using an Ed25519 SSH Key item held by 1Password and Unix `SSH_AUTH_SOCK`; explain references, fingerprints, per-ID selection, configuration-relative storage directly in `.psst/`, metadata visibility, commit-safe ciphertext, private local permissions, input/file limits, browser creation and separate environment mapping saves, linking existing references without Ctrl+U reuse, updates/deletion, and fail-stop command/export behavior. Drafts are masked by default; Ctrl+R toggles revelation with a 30-second expiry without fetching existing ciphertext. Only advertise integrations actually verified; allow compatible agents without vendor detection.
- [ ] Publish the implemented v1 format/challenge/authenticated-metadata encoding, hash-safe filenames, dependency rationale and compatibility/security checks, fresh-write randomness, validation rules, operation-local deduplication, serialized signing, and `PSST_TIMEOUT` queue-inclusive phase semantics. Update provider extension documentation to reflect the implemented interfaces without inventing an agreed public API.
- [ ] Explain the threat boundary: agent signing grants durable decryption capability and authorized processes can retain signatures/derived keys after locking; derived keys exist in psst memory. Recommend per-operation confirmation, explain approval frequency depends on settings, and warn against untrusted forwarding. Best-effort zeroization does not protect a compromised process; no guaranteed forwarded-agent detection is promised.
- [ ] Document recovery and manual migration while the old SSH key still works: preserve the key in 1Password, create new `(ID,key)` records, and update explicit selectors as needed. Lost keys lose decryptability; rotation cannot revoke old Git history or captured derived keys. Explain accepted lack of metadata hiding and replay/rollback protection against valid history.
- [ ] Document conflict and partial-failure recovery with independent browser storage creation before separate configuration linking, and configuration-before-storage deletion outcomes, including post-replacement errors, unused ciphertext, and failed ciphertext removal. Never promise cross-file atomic rollback.
- [ ] Preserve exclusions: no noninteractive/bare storage commands, standalone store browser, management import/export, automatic orphan cleanup/migration/rotation, recovery recipients, multi-recipient sharing, other key types, private-key reads, encryption keys fetched via `op`, custom primitives/plugins, agent discovery/startup/retries, Windows support, or changed legacy provider failure policy. Existing runtime `.env` export is not management export.
- [ ] Audit the automated acceptance matrix against the source: fake-agent/crypto-format/provider-consumer/fail-stop/operation/persistence/UI/PTY coverage must be present, including no-sign prevalidation, queue/cancellation/refetch counts, tamper/size/symlink failures, conflicts, partial outcomes, exact text, default masking and timed deliberate reveals, browser creation/separate linking, and terminal restoration. Fill demonstrable coverage gaps without merely duplicating tests from prior tickets. Normal tests/CI never contact personal keys/agents.
- [ ] Run and record repository Mise formatting, formatting check, locked all-targets Clippy with warnings denied, and locked tests. Explicitly verify Rust 1.88 and selected dependency compatibility; stable CI is insufficient MSRV evidence. Require tests/release builds on Linux/macOS x86_64 and ARM64, including Linux musl binaries; keep the existing release checks intact.
- [ ] Provide reproducible consented, opt-in real 1Password end-to-end checks on Linux and macOS for creation, later decrypt/update, approval/rejection, and verified repeatability. Record completed results and actual integration identification; the socket path alone is not proof of 1Password custody. If consent/access/platforms are unavailable, record the unmet release gate rather than mark readiness complete.
- [ ] Explicitly label the feature experimental until independent crypto/format review establishes production readiness. Record review evidence and follow-up findings when available, or clearly leave that production gate outstanding. Neither the source's authorized toy round-trip/tag test nor implementation self-tests are independent review or completed integration evidence. Experimental release still requires the other agreed automated, documentation, and live-integration gates.

## Evidence scope — release blocked

**Not release-ready or production-approved.** This ticket owns the merged readiness
inventory and verification evidence. See [README setup and usage](../../../README.md)
and [architecture](../../ARCHITECTURE.md) for current format, agent/security
boundary, persistence, provider interfaces, and recovery documentation; these
are not competing contracts in this ticket.

Current execution reflects direct `.psst/` storage, metadata browsing, encrypted
creation independent of environment mappings, separate reference selection/save,
and timed deliberate draft revelation. The spec incorporates the user-approved
amendments; the source idea and completed tickets retain the original nested
storage and workflows as historical context. This alignment does not rewrite
past approvals or ticket statuses. Older
`.psst/local-storage/` artifacts are not automatically migrated or read.

The matrix below inventories current test assertions, **not a current passing
suite or independent spec/rules/crypto review**. No tests, external checks, or live
agents were run during this documentation consolidation. Historical results are
kept separately and do not establish verification of the changed workflows.

## Native macOS follow-up — 2026-10-15

User authorized direct implementation in the current checkout because the project's
subagent supervisor supports Linux only. Base: `d3c8e42f031edd213966ad851ffeec923679c290`,
branch `v2`; changes were uncommitted when tested. No independent spec/rules review was run.
This evidence applies to the working-tree changes from this session, not the base
commit alone. Ticket status and release acceptance checkboxes remain unchanged.

Platform: macOS 27.0.1 (26A434), ARM64 / `aarch64-apple-darwin`.
Mise selected Rust 1.98.1 and Cargo 1.98.1. All verification commands removed
inherited `SSH_AUTH_SOCK`; only explicit fake endpoints were used. Clipboard
workflows used isolated helper executables, not the user's desktop clipboard.
No real 1Password agent or key was contacted.

Implemented follow-up:

- Injected identity-listing future in the independent-creation UI test rather than
  starting environment-selected agent listing; no process-global environment mutation.
- Dashboard Ctrl+D ignores a selected loading entry, preserving generation,
  loading, configuration and deletion state.
- Dashboard Esc exits immediately after errors, with or without a filter;
  editor/browser Esc follows the existing view navigation. Prompt hints match.
- UI errors retain contextual cause chains, sanitized when rendered in the
  scrollable details panel; nested-context coverage checks hidden secret values.
- PTY Ctrl+Y covers dashboard, reference editor, browser, manual draft and encrypted
  draft: exact multiline/Unicode/whitespace bytes, helper failure, unchanged
  configuration/ciphertext, and no additional agent requests for draft-only copies.
- All-consumer fail-stop now permits identity listing while refusing, disconnecting,
  oversizing or appending trailing data at signing; each case asserts four signing
  requests and no command, export replacement or clipboard-helper execution.
- Valid legacy records are ignored for fetch/listing and left byte-identical by
  direct-storage writes, without migration.
- Authenticated invalid UTF-8/NUL plaintext is rejected after exactly one signature,
  without returning plaintext or retrying. Public fixture key material is used.
- Metadata-valid distinct-record documents accept 1,024 and reject 1,025 records;
  oversized existing documents fail loading/authoring preparation before signing.
  This count test is metadata validation, not authentication of every cloned record.
- Corrected the authoring timeout assertion to inspect `.psst/<key>.json`.

Commands below ran with prefix `env -u SSH_AUTH_SOCK mise exec --` and finite
600-second tool deadlines. The first focused UI run caught an assertion requiring
secret-bearing `Debug`; the assertion was corrected without adding that trait.
Subsequent focused suites passed. Final checks on unchanged application/test files:

| Command | Result |
| --- | --- |
| `cargo fmt` | Passed |
| `cargo fmt --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked -- --test-threads=1` | 192 passed, including 21 PTY workflows |
| `cargo test --locked` | 192 passed, including 21 PTY workflows |
| `cargo build --locked --release` | Passed, native macOS ARM64 |

Default-parallel test log: `/tmp/psst-macos-parallel-tests.log` (local, temporary).
Other command output is retained in the implementation session, not durable CI
artifacts. Historical counts elsewhere describe different revisions; they are not
substituted for the 192 tests observed here.

Rust 1.88 was initially unavailable; the subsequent pinned-toolchain verification
below supersedes that limitation. Native macOS x86_64 and Linux x86_64/ARM64
(including musl) remain unverified for these edits.
At this automated checkpoint both live 1Password protocols were outstanding;
the later consented macOS results are recorded below. Independent spec/rules and
documentation acceptance, Linux live checks, and independent crypto/format review
remain outstanding. Nothing here establishes experimental release readiness or
removes the experimental label.

### Pinned Rust 1.88.0 follow-up — 2026-10-15

User requested Rust 1.88 as the development toolchain. Mise and CI now pin
`1.88.0`; the global default was not changed. `mise install rust@1.88.0`
succeeded, and `mise exec -- rustc --version` confirmed 1.88.0. No application
or test code changed in this toolchain follow-up. All commands below used
`env -u SSH_AUTH_SOCK mise exec --` with finite 600-second tool deadlines:

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Passed |
| `cargo check --locked --all-targets` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked` | Failed: 91 library tests passed, one failed; subsequent suites did not run |
| `cargo test --locked --lib providers::local_storage::storage::tests::browser_deletion_reports_atomic_write_partial_outcomes -- --exact` | Passed in isolation |
| `cargo test --locked -- --test-threads=1` | 192 passed, including 21 PTY workflows |
| `cargo build --locked --release` | Passed, native macOS ARM64 |

The parallel failure was `browser_deletion_reports_atomic_write_partial_outcomes`
at storage.rs:569: `Local-storage key is locked; try again`, caused by OS error 35
(`Resource temporarily unavailable`). At this checkpoint the cause was unknown;
isolated/serial passes did not resolve it. The lock-lifetime investigation and
verification below supersede that blocker. CI pins remain configuration only,
not passing-run evidence for other platforms.

### Lock-lifetime correction — 2026-10-15

The original parallel failure did not recur in the first investigation rerun.
A deterministic duplicate-descriptor regression then reproduced the same OS error
35 with close-only lock cleanup: `flock` remains held until every descriptor for
the same open file description is closed, including a descriptor temporarily
inherited by a concurrent fork before exec. This demonstrates the lifetime bug;
the original failing process was not traced to identify a particular child.

Configuration saves and per-key storage mutations now share an owned `FileLock`
guard that explicitly unlocks on drop, including early errors and cancellation.
It is created only after successful acquisition, so failed acquisition cannot
unlock another owner. No lock retries, sleeps, test serialization, path changes,
or weakened contention/conflict checks were introduced.

`lock_release_does_not_wait_for_duplicated_descriptors` failed deterministically
before explicit unlocking and passed afterward. It checks retained duplicates,
real contention, failed acquisition, release, and a subsequent owner's lock.
`lock_releases_on_error_and_cancelled_future_with_duplicate_open` checks early
failure and dropping a polled pending future while duplicates remain open.
Existing persistence, concurrency, cancellation and partial-outcome tests remain.

On native macOS ARM64, pinned Rust 1.88.0, with inherited `SSH_AUTH_SOCK` removed:

| Command after `mise exec --` | Result |
| --- | --- |
| `cargo fmt` / `cargo fmt --check` | Passed |
| `cargo clippy --locked --all-targets -- -D warnings` | Passed |
| `cargo test --locked` | 194 passed, including 21 PTY workflows, default parallel execution |
| `cargo test --locked --lib`, 20 consecutive runs | All passed; 94 tests per run |
| `cargo check --locked --all-targets` | Passed |
| `cargo build --locked --release` | Passed |

Commands used finite tool deadlines (600 seconds for the full suite/checks,
180 seconds for the 20-run loop plus compilation/release checks). Repetition logs:
`/var/folders/7t/j9k9z93x7gj6hpcxrsmmd8sh0000gn/T/psst-lock-regression.A7q04L/`
(local temporary evidence, not CI artifacts). No real agent, private key, or
1Password operation was accessed. The previous failing run remains recorded
above rather than being overwritten by the subsequent passes. Changes were
uncommitted at this checkpoint; independent reviews/platform/live gates remained outstanding.

## Consented macOS 1Password session — 2026-10-15

**Functional checks passed; evidence limitations remain.** The user explicitly
allowed isolated live tests through the macOS 1Password SSH agent, with signing
restricted to the disposable Ed25519 test key below. No private key was read or
exported, and no personal records were modified. Normal tests/CI remain isolated.
The user confirmed the key was disposable and enabled the SSH agent.

- Host: macOS 27.0.1 ARM64. Native Rust 1.88.0 release build, psst 2.0.0-alpha.4,
  base `d3c8e42f031edd213966ad851ffeec923679c290` plus this session's uncommitted
  fixes. Binary SHA256: `810748da04b667265500bc8bfa98cc3791503048ffdafa5e4ea3ae3394e324e4`.
- Integration: `/Applications/1Password.app`, bundle version **8.12.40**, identifier
  `com.1password.1password`, signed by AgileBits Inc., Team ID `2BUA8C4S2C`.
  `codesign --verify --deep --strict` passed. Homebrew's earlier receipt said
  8.12.38; the observed bundle version is used, not the stale receipt. The About
  dialog version was not separately confirmed.
- Endpoint: `$HOME/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock`.
  Process inspection identified the running app at the above path; `lsof` tied
  this socket to its PID 82643 and the current user. Socket mode was 0600.
  This identifies the integration beyond merely recognizing a vendor-like path.
- Consented fingerprint: `SHA256:fi3OF5K2KYvE4U8eV16caYhs8ML2ri9+lagWyiRC1pw`.
  Explicit-endpoint `ssh-add -l` confirmed a matching 256-bit Ed25519 identity;
  other public identities were omitted from output. Signing used only this key.
- Private nonsymlinked test directory: `/private/tmp/psst-spec01-live.GzMPH7`,
  mode 0700; `.psst/` 0700 and per-key JSON/lock files 0600. Explicit `PSST_CONFIG`
  and `SSH_AUTH_SOCK`, `PSST_TIMEOUT=60`. A temporary Python launcher supervised
  existing psst TUI/runtime interfaces for at most 600 seconds, terminating on
  timeout with a five-second kill grace and restoring terminal attributes.
  Assistant-launched CLI checks also had 90-second tool deadlines.
- **Approval policy/session duration: unknown.** User selected “Not sure” when
  asked. Lock/unlock and an approval prompt were user-confirmed, but exact prompt
  counts and linking-time absence of prompts were not explicitly reported. Do not
  infer per-request authorization or a fixed prompt count from these results.

| Check | Observed evidence |
| --- | --- |
| Creation and verified repeatability | User completed TUI creation of `readiness-check`; its stored record matched the selected key. Successful creation traverses two strictly verified identical signatures over one challenge. This is live implementation evidence, not independent crypto review. |
| Fresh-process decrypt | Assistant ran a new psst process wrapping `sh -c 'test "$SECRET_1" = test-only'`, with inherited `SECRET_1` removed: exit 0, no value printed. User repeated it, reported exit 0, and confirmed prior lock/unlock plus an approval prompt. |
| Update | Same ID/key replaced with `updated-test-only`; observed fresh salt/nonce and unchanged configuration bytes. Fresh-process comparison exited 0. An initial leading-space ID created a distinct record; the user corrected this, and later inspection showed only the intended record. Removal of the accidental record was not directly observed. |
| Separate second mapping | `SECRET_2` linked to the same qualified reference. Ciphertext bytes/hash were unchanged. A new process with both inherited variables removed compared both against `updated-test-only`: exit 0, no values printed. |
| Refused wrapped command | Explicit `SSH agent refused signing`, exit 1; the owned marker file was absent, proving the command did not launch. |
| Refused exports | Explicit signing refusal and exit 1 for absent and existing `.env`; absent stayed absent, existing retained exact sentinel bytes. |
| Refused copy | Explicit signing refusal and exit 1. An isolated fake `pbcopy` would write a marker; it never ran. The desktop clipboard was not used. |
| Refused update and creation | User reported TUI signing-refusal outcomes for replacement with `must-not-save` and new ID `refusal-new`. Assistant verified unchanged configuration/ciphertext hashes and no new record. These were refusals, not timeouts. |
| Recovery | First recovery attempt was refused; user explained they had not approved it. A separately announced new operation was approved and exited 0 comparing the retained value against `updated-test-only`. No automatic retry. |
| Creation without mapping save | Earlier attempts included separate linking, as the user confirmed. Final `final-unlinked-check` creation added a third record; both prior records remained, no mapping referenced the new ID, and configuration hash stayed byte-identical. |
| Exit | User confirmed the terminal was normal after Ctrl+C. This was a supervised session whose launcher also restores terminal attributes; direct app-only restoration has separate automated PTY evidence. |

Artifact checkpoints (SHA256; only dummy values were used):

| Artifact checkpoint | SHA256 |
| --- | --- |
| Initial single-mapping configuration, unchanged through update/refusal | `be8c78754a3b0a64720382edecfeae0bc8ef38f8bbaf96c8598db4ecddbebcdf` |
| Initial ciphertext document | `afb8555cc4518573b0079b443547971e1a091fe24ac542645d00d6cdba2a1293` |
| Updated ciphertext, unchanged through refusal and second linking | `97def55ae443c27f3ca23f1d3204c11c6164ac1b505013967ca749482f1ac1fa` |
| Two mappings to the updated record | `5d6d72a6b6494809699117097a899292d64d2359070946186eb886a6f25be4f2` |
| Final creation-only configuration before/after | `32d0ed0025182f2338ea306a6ca80dfc28e53c2455ce200e6a3001e224bbbd72` |

The user authorized deletion of only the owned temporary test directory after
preserving this evidence. Cleanup is pending: an interactive shell was still
using its `.psst/` subdirectory as cwd. The user must leave that directory before
removal, then remove the disposable 1Password item only after ciphertext cleanup.
No test key has been deleted by the assistant. These observations do not satisfy
Linux live verification, other-platform builds, independent reviews, or complete
approval-settings documentation. Ticket 07 remains in-progress and experimental.

## Automated acceptance matrix — current inventory

Test names identify evidence without prescribing implementation organization.
Nested fault scenarios are not separate top-level tests. Passing tests establish
only exercised cases, not universal correctness, integration identity, or
production safety.

| Source acceptance area | Concrete test evidence | Limits / follow-up |
| --- | --- | --- |
| Standard agent, unsupported keys, malformed/refused/disconnected/oversized replies | `malformed_identity_lists_are_rejected`, `tampering_and_agent_failures_are_errors_without_fallback`; shared fake agent models Unix protocol and RSA alongside Ed25519 | Fake implementation only; no real custody identification. |
| Exact ID/reference parsing, per-ID zero/one/multiple matches, explicit key selection, shared/per-user values | `failures_stop_all_consumers_before_launch_or_export`, `identity_selection_exact_text_and_operation_local_deduplication`, `invalid_inputs_and_metadata_do_not_sign`; provider registry case-sensitivity regression | Includes another caller directory and another key with unrelated ID. |
| Command/export/copy fail-stop, inherited variables, mixed successes | `failures_stop_all_consumers_before_launch_or_export` uses launch, absent/existing `.env`, clipboard and `op` helper markers, including excess identity count and delayed signing | Four timeout reads assert exactly four signatures, no retry. |
| Successful exact text injection, export escaping/order/empty omission, clipboard behavior | `identity_selection_exact_text_and_operation_local_deduplication`; existing export, CLI and legacy-provider regressions | Clipboard and command/export outputs are plaintext by design. Legacy warning/omission and argv behavior remain separate regressions. |
| Deduplication, serialized signing, fresh operations/refetches | Identity-selection test counts qualified/unqualified duplicate fetches and repeated commands; `isolated_operation_cancellation_and_error_order` and its worker count distinct queued records; `local_storage_opt_in_errors_refetch_and_cancellation_restore_terminal` counts fresh refetches | Operation-local only; timing alone is not proof of no persistent cache. |
| Queue-inclusive deadlines, active/queued cancellation, no retry, subsequent usability | `deadlines_include_queue_time`, `cancellation_releases_active_and_queued_work`, `timeout_environment_semantics`, `deadlines_active_and_queued_cancellation_do_not_retry_and_next_operation_works`, `author_save_deadline_includes_queued_work_without_retry`, `author_post_sign_listing_failure_deadline_and_cancellation`, `save_deadline_runs_with_isolated_timeout_environment`, `cancelling_signing_releases_lock_and_preserves_completed_changes` | Isolated workers control process-global timeout/socket settings; normal tests never select a personal endpoint. |
| Crypto exact text, byte bounds/NUL, verified repeatability, invalid signatures, fresh randomness | `exact_text_round_trips_and_fresh_writes`, `invalid_inputs_and_metadata_do_not_sign`, `tampering_and_agent_failures_are_errors_without_fallback`, `repeatability_guard` | Empty, multiline, whitespace, Unicode and maximum value size exercised. Valid-but-nonrepeatable signing uses test-only Dalek hazmat. |
| Stable byte encoding and cross-library fixture | `stable_encoding_and_derivation_vector`, `stable_node_crypto_fixture_decrypts_through_fake_agent`; [fixture](../../../tests/fixtures/local-storage-v1.json) and [offline generator](../../../tests/fixtures/generate-local-storage-v1.mjs) | Public fixed seed only; interoperability vector is not independent crypto review. |
| Tamper/tag/ID/key/version, duplicate/unknown/malformed fields, positional arrays, sizes | `tampering_and_agent_failures_are_errors_without_fallback`, `strict_key_files_bound_reads_and_writes`, `positional_json_arrays_are_rejected_before_signing`, `invalid_inputs_and_metadata_do_not_sign` | Known-invalid metadata asserts no signatures; authenticated mutation fails after signing where required. |
| Filesystem bounds, permissions, symlinks, filename/key metadata; no-sign rejection | `malformed_oversized_metadata_and_permissions_fail_without_signing`, `symlinked_ancestors_directories_records_and_locks_are_refused`, `aggregate_size_refusal_preserves_whole_file`; runtime all-consumer failure test | Covers read/save/remove and post-load ancestor swap. Aggregate encoded-size failure preserves old file; pre-sign input validation is distinct from post-encryption aggregate-size validation. |
| Direct storage and metadata-only browser across stored keys, including unlinked records | `storage_is_direct_and_only_storage_requires_private_permissions`, `browser_lists_all_identities_as_qualified_metadata_without_agent`, `listing_rejects_invalid_formats_and_oversized_files`, `listing_rejects_unsafe_paths_and_permissions`, `listing_bounds_aggregate_bytes`, `listing_bounds_directory_entries_including_ignored_files` | Lists qualified references without signing/decryption or loaded keys; storage itself needs private modes, not the ordinary configuration directory. Bounded listing does not prove live-agent compatibility. |
| Lock contention, loaded-byte conflicts, atomic replacement, other IDs/users | `loaded_conflicts_and_lock_contention_do_not_sign_or_clobber`, `create_replace_remove_preserve_ids_users_and_private_permissions`, `change_while_signing_is_not_overwritten`, `readers_see_whole_documents_during_repeated_replacement` | Cooperative locking, not protection against a malicious concurrent process. |
| Persistence failure boundaries, creation/deletion order and stale outcomes | `failed_commit_tracks_actual_replacement_and_loaded_state`, `encrypted_creation_faults_report_actual_surviving_files`, `encrypted_cancelled_save_reports_completed_storage_without_touching_new_draft`, `deletion_persistence_boundaries_report_actual_surviving_files_and_order`; existing config/export atomic-write tests | Creation fault seam uses synthetic ciphertext; concrete encrypted writer covered separately. Deletion asserts all 25 success/failure boundary combinations. No cross-file rollback claim; independent browser creation covered below. |
| Key chooser, selectors during approval, exact masked input and replacement consent | `author_rechecks_records_added_during_signing`, `author_updates_preserve_ciphertext_on_failure`, `encrypted_editor_preserves_exact_text_and_is_masked_by_default`, `encrypted_input_limit_is_exact_bytes_including_newline_and_paste`, `encrypted_focused_fields_and_full_fingerprints_at_supported_sizes`, `encrypted_identity_selection_errors_and_stale_listing`, `replacement_requires_y_and_can_be_declined_without_writes` | Full fingerprints and consent controls exercised at supported small sizes; warnings count loaded-configuration references only. Update failures preserve ciphertext without retry/fallback. |
| Browser creation independent of environment mapping; separate linking; no Ctrl+U reuse | `dashboard_new_row_is_selectable_and_not_an_entry`, `local_browser_creation_is_independent_of_environment_mapping`, `local_browser_cancel_preserves_reference_and_returns_to_browser`, `local_browser_new_row_stays_available_when_filter_has_no_matches`, `encrypted_ctrl_u_is_inert`; PTY `encrypted_creator_ctrl_u_is_inert_and_save_lock_error_keeps_ciphertext`, `encrypted_browser_link_and_shared_update_preserve_ciphertext_and_other_users`, `encrypted_browser_link_config_failure_keeps_ciphertext_and_usable_terminal` | Creation returns to browser without saving configuration; reference selection requires a separate mapping save. Linking neither signs nor rewrites ciphertext, even with agent refusal/storage lock; config conflict preserves ciphertext. |
| Timed deliberate draft revelation without fetching | `encrypted_draft_reveal_matches_manual_toggle_and_expiry`, `editor_reveals_expire_in_editor_and_browser`, `literal_reveal_toggle_sets_and_clears_expiry`; PTY `encrypted_creator_reveal_toggle_expiry_and_hidden_save` | Default masking, Ctrl+R reveal/hide, 30-second expiry and hidden save preserve exact draft; reveal does not sign. Deliberate reveals remain in terminal history, so no blanket never-rendered claim. |
| Complete TUI creation/update/deletion, exact paste/NUL, masking/argv, errors/cancellation/restoration | PTY `encrypted_create_save_resolve_exact_text_and_nul_rejection`, `encrypted_create_save_resolve_with_nonexistent_config_parents`, `encrypted_creation_chooser_selector_replacement_and_conflict`, browser-link/update tests above, `encrypted_deletion_default_cancel_explicit_and_unresolved_restore_terminal`, `encrypted_creation_agent_errors_cancellation_and_config_failure_leave_usable_tty`, `encrypted_human_editing_outlives_deadline_save_timeout_is_recoverable`, `encrypted_creation_no_eligible_keys_is_a_usable_error_not_arbitrary_selection`, opt-in refetch test above | Representative PTY workflows supplement exhaustive UI states; argv inspection uses Linux `/proc` only. PTY finish checks terminal restoration. |
| Deletion default keep, explicit/unresolved selection, preservation, conflicts/stale outcomes | `deletion_selection_and_snapshot_safety`, `deletion_conflicts_locks_and_swapped_symlinks_keep_ciphertext`, `deletion_default_cancel_unresolved_and_stale_results_are_safe`, `deletion_long_name_reserves_consent_at_supported_sizes`; deletion PTY above | Selection/removal asserts zero signatures and unchanged other-user files; malformed/ambiguous identity keeps ciphertext. |
| No personal agent/private-key/`op` access in normal tests | Shared fake agent uses public deterministic seeds and explicit fake endpoints; subprocess helpers remove inherited `SSH_AUTH_SOCK`; authoring workers require fixture markers; runtime helper detects unintended `op` invocation | Agent-protocol boundary, not private-key files. Test markers are not an OS-wide filesystem-access audit. Historical verification also unset inherited socket. |

### Replaced tests versus historical coverage

The 2026-10-13 run below predates browser linking and timed draft reveals. Its
`encrypted_editor_preserves_exact_text_and_never_reveals` was replaced by
`encrypted_editor_preserves_exact_text_and_is_masked_by_default` plus reveal
coverage. `reuse_failures_never_rewrite_existing_ciphertext` and its reuse worker
were replaced by `author_updates_preserve_ciphertext_on_failure` and
`author_update_worker`. `reuse_config_failures_and_stale_results_do_not_claim_ciphertext_writes`
was replaced by `encrypted_ctrl_u_is_inert` and
`encrypted_cancelled_save_reports_completed_storage_without_touching_new_draft`.
Historical PTY `encrypted_reuse_and_shared_update_preserve_ciphertext_and_other_users`
and `encrypted_reuse_failures_keep_ciphertext_and_restore_usable_terminal` are now
`encrypted_browser_link_and_shared_update_preserve_ciphertext_and_other_users`
and `encrypted_browser_link_config_failure_keeps_ciphertext_and_usable_terminal`.
These are changed assertions/workflows, not merely renamed evidence of a current
passing run; the newer browser/reveal/lock-error cases need current verification.

## Selected dependency/source checks

The original ticket 01 selected-library checks and bounded public-source retrieval
on **2026-10-13** are consolidated here. Neither is a current full advisory review.
RustSec HTML extraction failed for two pages in the latter run, so canonical
advisory-db Markdown was retrieved instead. Published patched ranges were compared
with the selected locked graph, **not a complete current dependency audit**.

| Selected library / source | Historical ticket 01 check | 2026-10-13 public-source evidence / limitation |
| --- | --- | --- |
| aes-gcm 0.10.3; [security notes](https://docs.rs/aes-gcm/0.10.3/aes_gcm/) | NCC library audit and constant-time-multiplication caveat; library audit is not a review of psst's composition/format. x86_64/ARM64 targets, not new variable-time-multiply processor support. | Notes rechecked; portable constant-time behavior depends on constant-time multiplication. Native target coverage still required. |
| aes-gcm 0.10.3; [RUSTSEC-2023-0096](https://rustsec.org/advisories/RUSTSEC-2023-0096.html) | Failed in-place decryption plaintext exposure patched in selected 0.10.3; caller returns no plaintext on error. | [Canonical advisory](https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/aes-gcm/RUSTSEC-2023-0096.md): patched >=0.10.3. |
| ed25519-dalek 2.2.0; [RUSTSEC-2022-0093](https://rustsec.org/advisories/RUSTSEC-2022-0093.html) | Patched in >=2; production uses validated public keys, strict verification and weak-key rejection, not hazmat/local signing. | [Canonical advisory](https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/ed25519-dalek/RUSTSEC-2022-0093.md): patched >=2. |
| curve25519-dalek 4.1.3; [RUSTSEC-2024-0344](https://rustsec.org/advisories/RUSTSEC-2024-0344.html) | Patched in selected 4.1.3. | [Canonical advisory](https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/curve25519-dalek/RUSTSEC-2024-0344.md): patched >=4.1.3. |
| sha2 0.10.9; [RUSTSEC-2021-0100](https://rustsec.org/advisories/RUSTSEC-2021-0100.html) | Affects 0.9.7, not selected 0.10.9. | [Canonical advisory](https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/sha2/RUSTSEC-2021-0100.md): patched >=0.9.8. |
| rand_core 0.6.4; [RUSTSEC-2019-0035](https://rustsec.org/advisories/RUSTSEC-2019-0035.html), [RUSTSEC-2021-0023](https://rustsec.org/advisories/RUSTSEC-2021-0023.html) | Both patched before selected 0.6.4; production uses fallible OS randomness, not a seeded PRNG. | [2019 canonical advisory](https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/rand_core/RUSTSEC-2019-0035.md): >=0.4.2; [2021 canonical advisory](https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/rand_core/RUSTSEC-2021-0023.md): >=0.6.2. |
| hkdf 0.12.4 / getrandom 0.2.17 and established API families above | Selected stable RustCrypto/Dalek primitives, not custom AES/Ed25519/HKDF or a claim of newest major versions. AES 0.8.4/GHASH 0.5.1/POLYVAL 0.6.2 direct dependencies enable cleanup on AES-GCM's graph; existing canonical encoding support; no SSH/encryption executable. | RustSec-domain search, including HKDF/getrandom, did not yield a complete audit. Absence of search results is not evidence of no advisories. |
| Library MSRV / locked compatibility | Declared MSRVs: 1.56 for AES-GCM/AES/GHASH, 1.81 for Ed25519-Dalek. Ticket 01 recorded actual Rust 1.88 locked build/tests; declarations/newer compiler alone are insufficient. | Ticket 07's historical Rust 1.88 results below establish local locked-graph compatibility at that time, not security or untested-target compatibility. Normal/build feature tree excluded Dalek hazmat; AES/GHASH/POLYVAL cleanup enabled. |
| 1Password [setup](https://developer.1password.com/docs/ssh/get-started/), [agent eligibility](https://developer.1password.com/docs/ssh/agent/), [authorization model](https://developer.1password.com/docs/ssh/agent/security/) | Target setup, not verified compatibility: Ed25519 SSH Key item, eligible vault, Developer SSH Agent settings and explicit Unix socket. Vendor example paths are not detection or proof of custody. | Documentation retrieved, no live integration contacted. Describes custody, Unix setup and configurable approval duration, not live repeatability/compatibility of this format. |

No cargo-audit database scan was performed; no clean full-graph audit or blanket
dependency-security claim is made. Fake-agent tests, the public vector and
historical toy experiment cannot establish 1Password custody or actual approval
behavior, nor replace independent crypto/format review. Release gates remain.

## Historical implementation and verification — 2026-10-13

- Original base `d38ea70`; starting HEAD `c01c33e104cb8dda6150ccda38bdc35df1b13d31`,
  branch `implement/01-local-storage-2`. Reconstructed spec/idea, seven tickets,
  instructions and rules. Tickets 01–06 were done and untouched; ticket 07 moved
  to in-progress from a clean worktree. Checkboxes stayed unchecked; no self-review,
  children, staging, commits, pushes or merges were performed in that run.
- Then-current documentation covered target setup, format/security/recovery,
  create/reuse/update/delete and actual partial outcomes. Provider interfaces were
  documented as implementation seams, not a stable public/plugin API. Its nested
  storage and Ctrl+U reuse/never-reveal workflows are superseded for current use.
- Filled the demonstrable all-consumer gap by extending the existing runtime
  failure test with delayed signing and excessive identity count: no wrapped
  launch, no absent/existing `.env` write, no clipboard helper, exactly four
  requests for four operations, no retry. No production source, dependency,
  workflow or unrelated test changes were needed for that historical slice.
- Historical changed paths: `README.md`, `docs/ARCHITECTURE.md`,
  `docs/local-storage-v1.md`, new `docs/local-storage-readiness.md`, this ticket,
  and `tests/local_storage_runtime.rs`. These are provenance, not current
  documentation destinations or this consolidation's changed-file scope.

### Historical command/log evidence

Serial finite-timeout Mise checks on GNU/Linux x86_64, sole installed target
`x86_64-unknown-linux-gnu`; configured Rust/Cargo **1.99.0**, explicit minimum
Rust/Cargo **1.88.0**. No personal endpoints, keys or live 1Password operations;
no toolchains/dependencies installed or unsupported platform setups attempted.
Logs: `/tmp/psst-ticket07-verification/`, exact exits in `results.log`, versions
and target in `toolchains.log` (30s). Historical reproduction prefix:

```sh
env -u SSH_AUTH_SOCK \
  PATH="/nix/store/j1938gsbvbhkyz4kbmvjk56r479mkw3s-gcc-wrapper-16.2.0/bin:$PATH" \
  timeout <seconds>s mise exec -- <command>
```

| Command after `mise exec --` | Seconds | Log | Exit |
| --- | --- | --- | --- |
| `cargo fmt` | 180 | `fmt.log` | 0 |
| `cargo test --locked --test local_storage_runtime -- --test-threads=1` | 180 | `focused.log` | 0 |
| `cargo fmt --check` | 180 | `fmt-check.log` | 0 |
| `cargo clippy --locked --all-targets -- -D warnings` | 300 | `clippy.log` | 0 |
| `cargo test --locked -- --test-threads=1` | 600 | `test.log` | 0 |
| `cargo build --locked --release` | 600 | `release.log` | 0 |
| `cargo +1.88.0 fmt --check` | 180 | `msrv-fmt-check.log` | 0 |
| `cargo +1.88.0 check --locked --all-targets` | 300 | `msrv-check.log` | 0 |
| `cargo +1.88.0 clippy --locked --all-targets -- -D warnings` | 300 | `msrv-clippy.log` | 0 |
| `cargo +1.88.0 test --locked -- --test-threads=1` | 600 | `msrv-test.log` | 0 |
| `cargo +1.88.0 build --locked --release` | 600 | `msrv-release.log` | 0 |
| `cargo tree --locked -e normal,build,features` | 30 | `features.log` | 0 |

Both historical full suites: **135 passed**, including **14 PTY workflows**;
focused runtime: **5 passed**. No required toolchain-check failures/retries occurred
in that run. Earlier tickets retain their historical failure/correction records.
These are **not current test counts or current MSRV/release verification**.

Before/after code/test/Cargo/Mise/workflow manifests compared unchanged by
verification (`code-before.sha256`, `code-after.sha256`); authorized test extension
preceded snapshots. Each manifest SHA-256:
`c2051ea1d526ad3c6384abee6503133e20cdb609d107b0bc0efb09e8d8c0b01e`.
Offline Node fixture regeneration (10s, `vector.json`/`vector.log`) exited 0 and
matched the committed vector byte-for-byte. Whitespace checks caught a
documentation trailing space and ticket trailing blank line, both removed;
final `timeout 10s git diff --check` passed. Local relative-link checks passed
(10s), without agents. These were hygiene checks, not spec/rules review PASSes.
Existing CI required both macOS architectures and both Linux musl architectures;
unchanged workflows did not prove completed runs, and GNU builds did not satisfy
musl release gates.

## Consented real 1Password protocol

**Do not run without new explicit consent for the exact platform, agent and test
key.** The original protocol was unexecuted; the 2026-10-15 macOS consent, results,
and remaining evidence limits are recorded above. Linux remains unexecuted.
Normal tests/CI never run this protocol. Run separately on native Linux and macOS
with a dedicated disposable Ed25519 SSH Key item in 1Password, not personal production keys. Preserve that key
until all test ciphertext is removed. Use existing TUI/runtime interfaces, not
new management commands; setup is in the [README](../../../README.md).

1. Record OS/architecture, psst commit/build/target, 1Password desktop version and
   installation type, exact consent scope and authorization settings. Identify
   the running vendor application and selected endpoint through its settings and
   OS process/socket ownership; compare the chooser fingerprint with the SSH Key
   item. A vendor-looking path alone is insufficient. If implementation/custody
   cannot be established, record the gate unmet.
2. In a fresh private nonsymlinked temporary directory, select its `psst.json`
   with `PSST_CONFIG`, explicitly select the intended `SSH_AUTH_SOCK`, and set
   `PSST_TIMEOUT=30` or record a positive approval budget. Open psst with a finite
   supervisory deadline suitable for manual editing. Without GNU timeout, use an
   equivalent finite supervisor; do not install tools just to claim this check.
3. Open **+ New entry** (Enter or Ctrl+N), select `local-storage`, then **Ctrl+B**.
   In the browser choose **+ New value** (Enter or Ctrl+N), enter ID
   `readiness-check`, dummy masked value `test-only`, select the dedicated
   fingerprint, Ctrl+S and approve. Creation requires two identical strictly
   verified signatures over the same v1 challenge; record successful creation as
   repeatability evidence, not merely identity listing. Record prompt/settings;
   check ciphertext was saved directly in `.psst/` and configuration was unchanged.
   Back in the browser, select the new qualified reference with Enter, set the
   environment name to `CHECK`, then **separately Ctrl+S** to save the mapping.
   Keep plaintext hidden; verify it is absent from config/ciphertext. An optional
   Ctrl+R check must occur on the dummy creation draft before its storage save:
   it reveals only the draft, toggles hide, and expires after 30 seconds without
   fetching. Record deliberate terminal-history exposure separately rather than
   claiming never rendered.
4. In a **new process**, run `psst sh -c 'test "$CHECK" = test-only'` under a finite
   supervisor, approve and record exit 0 without printing the value. Repeat after
   locking/unlocking 1Password with recorded settings; do not assume a fixed
   prompt count. This verifies later decryption, not creating-process caching.
5. Reopen CHECK's editor, Ctrl+B, then **+ New value** or Ctrl+N in the browser;
   enter the same ID and dedicated key with dummy `updated-test-only`, Ctrl+S,
   confirm replacement with `y`, approve and return to the browser. Confirm fresh
   salt/nonce and unchanged record count; existing mappings already reference the
   updated record. Select the reference and save the mapping separately if needed.
   Repeat the new-process check with the updated expected value. For a second
   environment mapping, open a new entry, select `local-storage`, **Ctrl+B and
   select the existing qualified reference**, then separately Ctrl+S. Verify
   byte-identical ciphertext across linking and both mappings' updated values
   without printing them. There is **no Ctrl+U reuse action**; linking does not
   request signing/decryption or rewrite ciphertext.
6. Configure per-request approval or reset authorization as vendor documentation
   permits. Reject a fresh decrypt: a wrapped marker command must not launch,
   `psst --write` must leave absent/existing `.env` unchanged, and copy must fail.
   Reject browser creation before any linking: no new ciphertext or mapping.
   Reject an update: old ciphertext and mappings remain unchanged. Rejection must
   be genuine refusal, not merely timeout; record the unmet case if it cannot be
   elicited. Linking an existing reference is not a signing-refusal test.
7. After refusal, retry explicitly as a new operation and approve. Check usable
   UI and restored terminal on exit. Record exact observed results, diagnostics
   without secrets, artifacts, prompts and finite budgets for each step. Clean
   only owned dummy config/ciphertext/export and the test item after verification;
   do not remove unrelated records/keys or retire the key before ciphertext cleanup.

Required results: creation, later decrypt/update, approval, rejection and verified
repeatability on **both Linux and macOS**. The macOS functional results above
are bounded evidence, not blanket integration or release approval. Linux remains
**NOT RUN**, and macOS authorization settings are still undocumented.

## BLOCKERS — do not mark done

- Native macOS ARM64 tests/release build passed in the 2026-10-15 follow-up above.
  macOS x86_64 and Linux x86_64/ARM64 (including musl) tests/release builds remain
  unverified for those changes. Rust 1.88 native macOS ARM64 parallel tests and
  release build passed after the lock-lifetime correction documented above.
  Historical GNU/Linux builds do not satisfy current musl or other-platform gates; CI
  configuration is not evidence of completed runs. Release checks remain intact.
- Consented live macOS ARM64 functional checks passed as recorded above, including
  integration identification, creation, decrypt/update, approval/refusal and
  repeatability. Authorization policy/session duration remain unknown; do not
  claim complete protocol documentation. Linux live checks remain **NOT RUN**
  without platform access and separate consent. macOS results do not satisfy Linux.
- Independent crypto/format production-readiness review absent; no review evidence
  or follow-up findings can be recorded. Feature remains explicitly experimental,
  and unmet platform/live gates also block experimental release readiness.
- Independent ticket 07 spec/rules reviews remain pending, intentionally not run
  here. This matrix and verification evidence are not review PASS claims.

All acceptance checkboxes remain unchecked and status remains **in-progress**.
Historical verification cannot waive current automated/documentation gates or
justify release readiness, additional verified integrations, or production approval.
