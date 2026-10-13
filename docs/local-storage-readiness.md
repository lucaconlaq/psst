# Experimental local-storage readiness record

Source: [spec](specs/01-local-storage.md), [ticket 07](tickets/local-storage/07-experimental-release-readiness.md).
Setup, format, security boundary and recovery: [v1 contract](local-storage-v1.md).

**Not release-ready or production-approved.** Experimental labeling is mandatory,
not a substitute for the agreed automated, platform and live-integration gates.
Ticket 07 remains in-progress. This is an implementation-time coverage inventory
and verification record, not an independent spec/rules or crypto review.
Original base: `d38ea70`; tickets 01–06 are committed through `c01c33e`.

## Automated acceptance matrix

Inventory of existing test assertions, plus ticket 07's narrow additions. Paths
below are relative to the repository root. Nested fault scenarios are not separate
top-level tests. Passing tests establish only the exercised cases, not universal
correctness, integration identity or production safety.

| Source acceptance area | Concrete test evidence | Limits / follow-up |
| --- | --- | --- |
| Standard agent, unsupported keys, malformed/refused/disconnected/oversized replies | `tests/local_storage.rs`: `malformed_identity_lists_are_rejected`, `tampering_and_agent_failures_are_errors_without_fallback`; shared `tests/support/agent.rs` models Unix protocol and RSA alongside Ed25519 | Fake implementation only; no real custody identification. |
| Exact ID/reference parsing, per-ID zero/one/multiple matches, explicit key selection, shared/per-user values | `tests/local_storage_runtime.rs`: `failures_stop_all_consumers_before_launch_or_export`, `identity_selection_exact_text_and_operation_local_deduplication`; `tests/local_storage.rs`: `invalid_inputs_and_metadata_do_not_sign` | Includes another caller directory and another key with unrelated ID. Registry case sensitivity is tested in `src/providers/mod.rs`. |
| Command/export/copy fail-stop, inherited variables, mixed successes | Runtime `failures_stop_all_consumers_before_launch_or_export` uses a launch marker, absent/existing `.env`, and clipboard/`op` helper markers for each scenario; identity test includes ambiguity/missing eligible keys | Ticket 07 adds excess identity count and delayed signing to this existing all-consumer helper. Four timeout reads produce exactly four signatures, no retry. |
| Successful exact text injection, export escaping/order/empty omission, clipboard behavior | Runtime `identity_selection_exact_text_and_operation_local_deduplication`; `tests/export.rs`, `tests/cli.rs`, `tests/legacy_providers.rs` | Clipboard and command/export outputs are plaintext by design. Legacy warning/omission and argv behavior remain separate regressions. |
| Deduplication, serialized signing, fresh operations/refetches | Runtime identity test counts qualified/unqualified duplicate fetches and repeated commands; `isolated_operation_cancellation_and_error_order` and its worker count distinct queued records; terminal `local_storage_opt_in_errors_refetch_and_cancellation_restore_terminal` counts fresh refetches | Operation-local only; no persistent cache evidence inferred from timing alone. |
| Queue-inclusive deadlines, active/queued cancellation, no retry, subsequent usability | `tests/local_storage.rs`: `deadlines_include_queue_time`, `cancellation_releases_active_and_queued_work`, `timeout_environment_semantics`; runtime `deadlines_active_and_queued_cancellation_do_not_retry_and_next_operation_works`; authoring `author_save_deadline_includes_queued_work_without_retry`, `author_post_sign_listing_failure_deadline_and_cancellation`; concurrency `save_deadline_runs_with_isolated_timeout_environment` | Isolated workers control process-global timeout/socket settings; normal tests never select a personal endpoint. |
| Crypto exact text, byte bounds/NUL, verified repeatability, invalid signatures, fresh randomness | `tests/local_storage.rs`: `exact_text_round_trips_and_fresh_writes`, `invalid_inputs_and_metadata_do_not_sign`, `tampering_and_agent_failures_are_errors_without_fallback`; `src/providers/local_storage/mod.rs`: `repeatability_guard` | Empty, multiline, whitespace, Unicode and maximum value size are exercised. Valid-but-nonrepeatable fault signing is test-only Dalek hazmat. |
| Stable byte encoding and cross-library fixture | Module `stable_encoding_and_derivation_vector`; `stable_node_crypto_fixture_decrypts_through_fake_agent`; `tests/fixtures/generate-local-storage-v1.mjs` | Public fixed test seed only; interoperability vector is not independent crypto review. |
| Tamper/tag/ID/key/version, duplicate/unknown/malformed fields, positional arrays, sizes | Local-storage `tampering_and_agent_failures_are_errors_without_fallback`, `strict_key_files_bound_reads_and_writes`, `positional_json_arrays_are_rejected_before_signing`, `invalid_inputs_and_metadata_do_not_sign` | Known-invalid metadata asserts no signatures; authenticated mutation fails after signing where required. |
| Filesystem bounds, permissions, symlinks, filename/key metadata; no-sign rejection | `tests/local_storage_persistence.rs`: `malformed_oversized_metadata_and_permissions_fail_without_signing`, `symlinked_ancestors_directories_records_and_locks_are_refused`, `aggregate_size_refusal_preserves_whole_file`; runtime all-consumer failure test | Covers read/save/remove and post-load ancestor swap. Aggregate encoded-size failure preserves old file; input/pre-sign validation is distinct from post-encryption aggregate-size validation. |
| Lock contention, loaded-byte conflicts, atomic replacement, other IDs/users | Persistence `loaded_conflicts_and_lock_contention_do_not_sign_or_clobber`, `create_replace_remove_preserve_ids_users_and_private_permissions`; concurrency `change_while_signing_is_not_overwritten`, `readers_see_whole_documents_during_repeated_replacement` | Cooperative locking, not a guarantee against a malicious concurrent process. |
| Persistence failure boundaries and creation/deletion order | Storage module `failed_commit_tracks_actual_replacement_and_loaded_state`; UI `encrypted_creation_faults_report_actual_surviving_files`, `reuse_config_failures_and_stale_results_do_not_claim_ciphertext_writes`, `deletion_persistence_boundaries_report_actual_surviving_files_and_order`; existing config/export atomic-write tests | UI creation fault seam uses synthetic ciphertext; concrete encrypted writer is covered separately. Deletion asserts all 25 success/failure boundary combinations. No cross-file rollback claim. |
| Key chooser, selectors during approval, masked input, shared replacement consent | `tests/local_storage_authoring.rs`: `author_rechecks_records_added_during_signing`, `reuse_failures_never_rewrite_existing_ciphertext`; UI `encrypted_editor_preserves_exact_text_and_never_reveals`, `encrypted_input_limit_is_exact_bytes_including_newline_and_paste`, `encrypted_focused_fields_and_full_fingerprints_at_supported_sizes`, `replacement_requires_y_and_can_be_declined_without_writes` | Full fingerprints and consent controls exercised at supported small sizes; warnings count loaded-config references only. |
| TUI create/reuse/update/delete complete workflows, exact paste/NUL, masking/argv, stale/cancel/errors/restoration | `tests/terminal.rs`: `encrypted_create_save_resolve_exact_text_and_nul_rejection`, `encrypted_create_save_resolve_with_nonexistent_config_parents`, `encrypted_creation_chooser_selector_replacement_and_conflict`, `encrypted_reuse_and_shared_update_preserve_ciphertext_and_other_users`, `encrypted_reuse_failures_keep_ciphertext_and_restore_usable_terminal`, `encrypted_deletion_default_cancel_explicit_and_unresolved_restore_terminal`, agent-error/no-key/timeout workflows and opt-in refetch workflow | PTY representative workflows supplement exhaustive UI states; argv inspection uses Linux `/proc` only. PTY `finish` checks terminal restoration. |
| Deletion default keep, explicit/unresolved selection, preservation, conflicts and stale outcomes | `tests/local_storage_deletion.rs`: `deletion_selection_and_snapshot_safety`; UI `deletion_conflicts_locks_and_swapped_symlinks_keep_ciphertext`, `deletion_default_cancel_unresolved_and_stale_results_are_safe`, `deletion_long_name_reserves_consent_at_supported_sizes`; deletion PTY above | Selection/removal asserts zero signatures and unchanged other-user files; malformed/ambiguous identity keeps ciphertext. |
| No personal agent/private-key/`op` access in normal tests | Shared agent uses public deterministic test seeds and explicit `Agent::at`; subprocess helpers remove inherited `SSH_AUTH_SOCK` before optionally selecting fake sockets; authoring workers require their fixture marker; runtime helper detects unintended `op` invocation | Code boundary uses agent protocol, not private-key files. Test markers are not an OS-wide filesystem-access audit. All verification below also unsets inherited socket. |

No production source changes were needed for this slice. The demonstrable
all-consumer timeout/count gap was filled without duplicating earlier workflows.
This inventory does not replace the independent reviews required by the workflow.

## Selected dependency/source checks

Fresh public-source retrieval on **2026-10-13** rechecked the AES-GCM 0.10.3
[security notes](https://docs.rs/aes-gcm/0.10.3/aes_gcm/): its NCC library audit
is not a review of psst's composition, and portable constant-time behavior depends
on constant-time multiplication. Native target coverage is still required.

RustSec HTML extraction failed for two pages; canonical advisory-db Markdown was
retrieved instead. Bounded checks compare these published patched ranges with the
selected locked graph; they are **not a complete current dependency audit**:

| Selected crate | Retrieved advisory | Patched range |
| --- | --- | --- |
| aes-gcm 0.10.3 | [2023-0096](https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/aes-gcm/RUSTSEC-2023-0096.md) | >=0.10.3 |
| ed25519-dalek 2.2.0 | [2022-0093](https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/ed25519-dalek/RUSTSEC-2022-0093.md) | >=2 |
| curve25519-dalek 4.1.3 | [2024-0344](https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/curve25519-dalek/RUSTSEC-2024-0344.md) | >=4.1.3 |
| sha2 0.10.9 | [2021-0100](https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/sha2/RUSTSEC-2021-0100.md) | >=0.9.8 |
| rand_core 0.6.4 | [2019-0035](https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/rand_core/RUSTSEC-2019-0035.md), [2021-0023](https://raw.githubusercontent.com/RustSec/advisory-db/main/crates/rand_core/RUSTSEC-2021-0023.md) | >=0.4.2 and >=0.6.2 respectively |

A fresh RustSec-domain search for these crates plus HKDF/getrandom did not provide
a complete audit; absence of search results is not evidence of no advisories.
No cargo-audit database scan was performed; no clean full-graph audit is claimed.
These selected-source checks cannot establish a blanket dependency-security claim.
The recorded Rust 1.88 build/tests below establish local locked-graph compatibility,
not security or compatibility on untested targets. The normal/build feature tree
excludes Dalek hazmat; AES/GHASH/POLYVAL cleanup features are enabled.

Current [1Password setup](https://developer.1password.com/docs/ssh/get-started/),
[agent eligibility](https://developer.1password.com/docs/ssh/agent/) and
[authorization model](https://developer.1password.com/docs/ssh/agent/security/)
were retrieved for documentation, not contacted as a live integration. They describe
Ed25519 SSH Key custody, Unix socket setup, and configurable approval duration;
they do not establish this format's live signing repeatability or compatibility.

## Local verification

Logs: `/tmp/psst-ticket07-verification/` (`results.log`, `toolchains.log` and names
below). GNU/Linux x86_64, target `x86_64-unknown-linux-gnu`, configured Rust/Cargo
**1.99.0**, explicit minimum Rust/Cargo **1.88.0**. Commands ran **serially**, with
finite GNU timeout budgets, no installs, and inherited `SSH_AUTH_SOCK` unset.
Reproduction prefix:

```sh
env -u SSH_AUTH_SOCK \
  PATH="/nix/store/j1938gsbvbhkyz4kbmvjk56r479mkw3s-gcc-wrapper-16.2.0/bin:$PATH" \
  timeout <seconds>s mise exec -- <command>
```

| Command | Seconds | Log | Exit |
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

Both full suites passed **135 top-level tests**, including **14 PTY tests**;
focused runtime passed 5. No required-check failures or retries occurred in this
run. Earlier tickets retain their historical failure/correction records.
Offline Node vector regeneration (10s, `vector.json`/`vector.log`) exited 0 and
matched the committed fixture. Code/test/Cargo/Mise/workflow manifests were
unchanged by verification; both SHA-256 manifest digests are
`c2051ea1d526ad3c6384abee6503133e20cdb609d107b0bc0efb09e8d8c0b01e`.
Local relative-link checks and final `git diff --check` passed (10s each).
Intermediate documentation whitespace failures are recorded in ticket 07;
no implementation or requirements changes resulted.
Existing CI still requires tests and release builds on both macOS architectures
and both Linux musl architectures; unchanged workflow configuration is not proof
those runs completed. Local GNU builds do not satisfy the Linux musl release gate.

## Consented real 1Password protocol (NOT RUN)

**Do not run without new explicit consent for the exact platform, agent and test
key.** No consent was supplied for ticket 07. Normal tests/CI never run this
protocol. Run separately on native Linux and macOS; use a dedicated disposable
Ed25519 SSH Key item in 1Password, not personal production keys. Keep the key until
all test ciphertext has been removed. These instructions use existing TUI/runtime
interfaces, not new management commands.

1. Record OS/architecture, psst commit/build/target, 1Password desktop version and
   installation type, explicit consent scope, and authorization settings. Identify
   the running vendor application and selected endpoint through its settings and
   OS process/socket ownership; compare the public fingerprint displayed in the
   psst chooser with the SSH Key item. A vendor-looking path alone is insufficient.
   If actual implementation/custody cannot be established, record the gate unmet.
2. In a fresh private, nonsymlinked temporary directory, set `PSST_CONFIG` to its
   `psst.json`, select the intended `SSH_AUTH_SOCK` using the setup instructions,
   and set `PSST_TIMEOUT=30` (or a recorded positive approval budget). Open psst in
   a terminal with a finite supervisory deadline appropriate for manual editing.
   On systems without GNU timeout, use an equivalent finite supervisor; do not
   install tools solely to claim this check here.
3. Add environment `CHECK`, source `local-storage`, Ctrl+N, ID `readiness-check`,
   dummy masked value `test-only`, choose the dedicated fingerprint, Ctrl+S and
   approve. Save success requires two identical strictly verified signatures over
   the same v1 challenge: record that result as repeatability evidence, not merely
   a successful identity listing. Record prompt count/settings and ensure no
   plaintext appeared in the UI/config/ciphertext. Exit the TUI.
4. In a **new process**, run `psst sh -c 'test "$CHECK" = test-only'` with a finite
   supervisor, approve, and record exit 0. Repeat after locking/unlocking 1Password
   under the chosen settings; do not assume a fixed prompt count. This verifies
   later decryption, not caching in the creating process.
5. Reopen CHECK, Ctrl+N, enter dummy `updated-test-only`, confirm replacement with
   `y`, approve and save. Repeat the new-process check with the new expected value;
   confirm fresh salt/nonce and that the record count did not grow. Reuse via
   Ctrl+U for a second mapping and verify byte-identical ciphertext plus both
   mappings' updated value without printing it.
6. Configure per-request approval or reset the authorization session as documented
   by the vendor. Reject a fresh decrypt request: a wrapped marker command must
   not launch; `psst --write` must leave absent/existing `.env` unchanged; copy
   must fail. Reject a new creation and an update: verify no mapping/new ciphertext
   or overwritten old ciphertext respectively. Rejection must be genuine refusal,
   not simply timeout; if the application cannot elicit it, record the unmet case.
7. After refusal, explicitly retry as a new operation and approve; verify the UI
   remains usable and terminal state is restored on exit. Record exact observed
   results, diagnostics without secrets, artifacts, prompt counts and finite
   budgets for each step. Clean only the owned dummy config/ciphertext/export and
   test item after verification. Do not delete unrelated records or keys.

Required results: creation, later decrypt/update, approval, rejection and verified
repeatability on **both Linux and macOS**. Every result is currently **NOT RUN**;
no 1Password integration is advertised as verified.

## BLOCKERS

- Native macOS x86_64/ARM64 and Linux ARM64 tests/release builds are unavailable
  here; Linux x86_64/ARM64 **musl** tests/release binaries remain unverified.
- Consented Linux/macOS live 1Password end-to-end checks above are unmet.
- Independent crypto/format production review is absent; self-tests, the public
  vector, historical toy experiment and ticket-scoped reviews cannot replace it.
- Independent ticket 07 spec/rules reviews have not been performed in this run.

Do not mark ticket 07 done, release the feature as ready, advertise additional
verified integrations, or relax these gates on the basis of this record.
