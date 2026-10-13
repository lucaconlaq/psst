# 02: Safely persist encrypted record changes

**Source:** [Agreed spec](../../../docs/specs/01-local-storage.md), including its linked idea and exclusions.

**What to build:** Independently verifiable creation, replacement, and removal of encrypted `(ID, public key)` records, protecting concurrent edits and accurately reporting what survived a failed save. Reuse ticket 01's agent/envelope contracts; do not add noninteractive management commands.

**Blocked by:** 01 — Verify SSH-agent encrypted round-trips and define v1

**Status:** done

- [x] First add the smallest behavior-preserving persistence prefactor needed to distinguish failure before replacement from failure after replacement/directory sync. Cover existing configuration and `.env` atomic-write behavior with regressions; do not invent cross-file transactions or change legacy semantics.
- [x] Store one JSON file per public key under `.psst/local-storage/` beside the selected configuration, using ticket 01's hash-safe filenames. Preserve unrelated IDs and other users' files; IDs never become paths.
- [x] Read/write at most 4 MiB per key file. Strictly validate the complete applicable format/key/path metadata before signing; malformed/duplicate/unknown-version/oversized data must not be silently replaced. Verify these failures issue no signing requests.
- [x] Refuse symlinked storage paths, including relevant storage directories, record and lock paths. Use private local directory/file permissions and verify them. Test symlink refusal on reads, writes, and removal rather than relying on the existing atomic writer alone.
- [x] Protect per-key mutations with locking, loaded-content conflict detection, and synced whole-file atomic replacement. Reject lock contention and concurrent same-key changes without clobbering; preserve every unrelated record. Validate serialized size before replacement.
- [x] Test create, replacement with fresh salt/nonce, and targeted removal followed by successful decryption of surviving records. Include multiple IDs, multiple users, missing files, conflicts, permissions, and whole-file atomicity.
- [x] Inject write, replacement, and sync-boundary failures; report whether content actually changed without claiming rollback or complete success. Ensure callers can honor storage-before-config creation and config-before-ciphertext deletion, including completed changes on cancellation; full two-stage workflow verification belongs to tickets 04 and 06.
- [x] Use fake agents only, apply ticket 01's save-phase deadline/cancellation and secret-lifetime rules, and keep existing configuration/export tests plus repository formatting/Clippy/tests green.

## Implementation evidence (awaiting independent reviews)

- Original base `d38ea70c001b12641f4021208ec91df668f9b565`; workflow HEAD
  `6f6f8fe6384c9a30e4adc5394b02a9d146c9b7a6`. Ticket 01's accumulated work and
  unrelated edits were preserved. Ticket 02 remains in-progress with unchecked
  criteria; no reviews, children, staging, commits, pushes, or merges occurred.
- The existing atomic writer now exposes `PersistenceError.changed` for errors
  before/after replacement; legacy configuration/export wrappers retain their
  persistence ordering and state-update semantics. Regression tests cover legacy
  configuration and `.env` output, private files, cleanup, and failed replacement.
- `StoredKey` loads config-relative, bounded, fully validated per-key documents;
  mutations use private directories/files, no-follow file opens, ancestor/record/
  lock symlink refusal, cooperative per-key locks, loaded-byte conflict detection,
  bounded serialization, and synced whole-file replacement. Newly created
  directory entries are synced too. Insecure existing permissions are refused,
  not silently repaired. IDs never become filesystem paths.
- Fake-agent regressions cover creation/fresh replacement/targeted removal,
  surviving-record decryption, multiple IDs/users, missing data, pre-sign invalid
  data rejection, permissions, symlinks on reads/writes/removal, lock contention,
  conflicts before/during signing, aggregate size rejection, and whole-document
  readers during replacement. Save timeout/cancellation releases locks and leaves
  earlier completed changes intact; subsequent saves/decryption succeed.
- Injected temporary-write, file-sync, replacement, and directory-sync failures
  prove old-or-complete-new artifacts and accurate replacement evidence. Storage
  updates its loaded state after completed replacement even when sync fails;
  callers can downcast errors to inspect `changed`. Synchronous commit has no
  cancellation await after signing. Creation can await storage before saving
  configuration; deletion can save configuration before removing ciphertext.
  No cross-file rollback is promised; full two-stage workflows remain later scope.
- Final serial verification all exited 0 on Linux x86_64: configured Mise
  `cargo fmt`, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`,
  `cargo test --locked` (98 tests), `cargo +1.88.0 test --locked` (98 tests), and
  `cargo +1.88.0 build --locked`. Logs: `/tmp/psst-ticket02-verification/`, named
  `fmt.log`, `fmt-check.log`, `clippy.log`, `tests.log`, `tests-1.88.log`, and
  `build-1.88.log`. Final checks passed first time; no failures were hidden.
- Every final command used `env -u SSH_AUTH_SOCK`, the recorded GCC wrapper
  `/nix/store/j1938gsbvbhkyz4kbmvjk56r479mkw3s-gcc-wrapper-16.2.0/bin` on PATH,
  and GNU `timeout`: 180 seconds for formatting/Clippy/build, 240 for configured
  tests, 300 for minimum-version tests. Fake sockets only; no personal-agent or
  private-key access. `git diff --check` passed. No implementation blockers;
  independent reviews are outstanding. No CLI, provider registration, TUI,
  later-ticket workflows, or release-readiness claims were added.

## Independent-finding correction evidence

- Supplied independent reports covered 26/26 files and otherwise completed:
  Spec blocker identified raw temporary fixture roots on macOS; Rules blockers
  identified import placement (RUST-2) and one comment width (RUST-1). This run
  fixes only those findings, preserving accumulated/unrelated work against base
  `d38ea70` and workflow HEAD `6f6f8fe`. No new reviews were performed.
- Canonicalized temporary configuration roots before storage loads, including
  persistence, concurrency, the isolated deadline worker, and the injected
  failure-boundary unit test. Deliberate symlink paths remain unresolved and
  refused; production checks are unchanged. This addresses Linux/macOS support
  in the source spec's **Implementation Decisions → Verified current state**
  without changing its symlink-refusal contract.
- Moved fake-agent imports before attributed out-of-line support declarations,
  retaining the attribute/comment, and wrapped the cooperative-lock comment.
  After these fixes, Linux focused tests passed with `TMPDIR` set to a symlink
  (`/tmp/psst-ticket02-corrections/alias-tmp`): 10 integration tests, including
  the deadline subprocess and deliberate symlink refusals, plus the storage
  failure-boundary unit test. This reproduces the ancestor-path condition,
  not a native macOS validation; macOS is unavailable here.
- Configured Mise `cargo fmt`, `cargo fmt --check`, locked all-target Clippy
  with `-D warnings`, and `cargo test --locked` passed. Rust 1.88.0 locked full
  tests and locked build also passed. Both full suites passed 98 tests on their
  first runs, reproducing the previously reported Linux evidence; no required
  check failed. Logs: `/tmp/psst-ticket02-corrections/` (`fmt.log`, `fmt-check.log`,
  `clippy.log`, `tests.log`, `tests-1.88.log`, `build-1.88.log`,
  `focused-symlink-tmp.log`, `unit-symlink-tmp.log`).
- Every toolchain command used `env -u SSH_AUTH_SOCK`, `mise exec --`, and the
  ticket 01 GCC-wrapper PATH. GNU timeout budgets: 180 seconds for formatting,
  focused tests, Clippy, and build; 240 for configured full tests; 300 for
  Rust 1.88 full tests. Fake agents only; no personal-agent/private-key access.
  `timeout 10s git diff --check` and an AWK comment-width check passed. An
  optional Python comment scan could not start (`python3` unavailable); AWK
  supplied that check instead. Ticket 02 stays in-progress with
  unchecked criteria. No requirements changes, new reviews/children, staging,
  commits, pushes, or merges. Native macOS/platform release validation and the
  spec's live-integration/production-readiness gates remain outstanding.

## Completion evidence

- Latest independent **Spec PASS**: `/tmp/psst-subagent-rA2cOk`; **Rules PASS**:
  `/tmp/psst-subagent-ugMFf3`. Both reports fully covered 26/26 files, had no
  actionable findings, and ended with exit code 0 and settled, non-aborted events.
  These are ticket-scoped static reviews, not full-spec or production approval.
- Rechecked all eight criteria against persistence/format/agent code and the
  regressions: legacy atomic-write behavior; config-relative hash-safe files;
  bounded complete validation before signing; private permissions and symlink
  refusal; locking/conflicts and bounded synced replacement; fresh replacement,
  targeted removal and surviving IDs/users; injected pre/post-replacement failure
  evidence and cancellation; fake-agent deadlines and operation-local secrets.
  Full two-stage creation/deletion workflows remain tickets 04/06 scope.
- Completion reran required checks serially on Linux x86_64, all exit 0:
  `mise exec -- cargo fmt`, `cargo fmt --check`,
  `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`
  (98 passed), `cargo +1.88.0 test --locked` (98 passed), and
  `cargo +1.88.0 build --locked`. Every cargo command used `mise exec --`.
- Symlink-`TMPDIR` reruns passed: locked integration suites
  `local_storage_persistence` and `local_storage_concurrency` (10 tests, including
  deadline subprocess and deliberate symlink refusals), then the locked library
  `failed_commit_tracks_actual_replacement_and_loaded_state` test (1 passed).
  TMPDIR was `/tmp/psst-ticket02-completion-KqXblQ/alias-tmp`, linked to `real-tmp`.
- Logs: `/tmp/psst-ticket02-completion-KqXblQ/` — `fmt.log`, `fmt-check.log`,
  `clippy.log`, `tests.log`, `tests-1.88.log`, `build-1.88.log`,
  `focused-symlink-tmp.log`, `unit-symlink-tmp.log`, and `statuses.log`.
  Commands used `env -u SSH_AUTH_SOCK` and ticket 01's GCC-wrapper PATH:
  `/nix/store/j1938gsbvbhkyz4kbmvjk56r479mkw3s-gcc-wrapper-16.2.0/bin`.
  GNU timeout budgets were 180 seconds each except configured full tests
  (240 seconds) and Rust 1.88 full tests (300 seconds). Fake agents only;
  no personal-agent or private-key access. No failed required checks.
- `before.sha256` and `after-verification.sha256` match for all tracked and
  non-ignored untracked files: verification changed no code, tests, or other
  files. `timeout 10s git diff --check` passed. Completion changes only this
  ticket's status, checkboxes, and evidence; unrelated accumulated work is intact.
  Base remains `d38ea70`, workflow HEAD `6f6f8fe`. No new reviews/children,
  requirements changes, staging, commits, pushes, or merges.
- Ticket 02 is done with no blockers. Lifecycle reconciled without dependent
  edits: 03 remains ready-for-agent (01 done); 04 and 06 remain blocked by 03,
  retaining their 02/03 dependency references. 05 and 07 remain blocked.
  Native macOS, ARM64/musl/full release builds, real 1Password integration,
  and production-readiness gates are not claimed.
