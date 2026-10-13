# 03: Resolve encrypted references safely across runtime consumers

**Source:** [Agreed spec](../../../docs/specs/01-local-storage.md), including its linked idea and exclusions.

**What to build:** Users can inject, export, copy, or explicitly fetch their encrypted values through the exact `local-storage` source, with identity-safe selection, operation-local deduplication, and fail-stop command/export behavior. Use ticket 01's valid fixtures for end-to-end tests; authoring is not required to verify this slice.

**Blocked by:** 01 — Verify SSH-agent encrypted round-trips and define v1

**Status:** done

- [x] Register the exact case-sensitive source `local-storage`, with provider-owned metadata. Preserve ordered string `source`/`value` configuration and discovery (`PSST_CONFIG`, nearest visible/hidden config). Read `.psst/local-storage/` beside the chosen config, never beside the caller's cwd.
- [x] Parse exactly `secret-id` or `secret-id#SHA256:fingerprint` using OpenSSH SHA-256 public-key fingerprints and ticket 01's ID rules. Unqualified references require exactly one agent key with a stored value for that ID, not a key merely present elsewhere in storage. Qualified references select only that key for that entry.
- [x] Missing keys/records, unsupported keys, ambiguity, agent refusal/disconnection/malformed responses, validation failures, and decryption/authentication failures are errors with no other-identity or plaintext fallback. Enforce 4 MiB reads, key/path matching, private local permissions, and symlink refusal before signing where applicable.
- [x] Every `local-storage` resolution failure prevents wrapped-command launch and `.env` creation/replacement. CLI tests must cover each failure class with subprocess markers and filesystem assertions, including mixed successes and inherited variables. Other providers keep warning/omission behavior.
- [x] Verify successful injection/export and exact empty/multiline/Unicode text under existing consumer rules: empty is a valid provider result, injection accepts it, and export still omits successful empty values. Copy failures must not launch a clipboard helper; retain existing empty-copy behavior.
- [x] Deduplicate identical resolved `(ID,key)` fetches within one operation, including qualified/unqualified references resolving to the same record. Keep plaintext/derived keys operation-local, discard after use, and best-effort zeroize; there is no persistent cache. A single fetch and each TUI refetch start fresh operations.
- [x] Count fake-agent requests for shared mappings, per-user values, and separate operations. Verify serialized signing, deadlines including queue time, active/queued cancellation, no retries/fallback, and usable subsequent operations. Preserve legacy provider concurrency, config order, individual errors, and the existing `op` queue policy.
- [x] TUI opt-in fetches show per-entry failures and remain usable; preserve cancellation, stale-result rejection, short-lived deliberate reveals, and terminal restoration. Prove fresh refetches request the agent again rather than reuse a persistent secret cache.
- [x] Test config-relative resolution from another caller directory, explicit selection with multiple loaded keys, per-ID zero/one/multiple matches, shared mappings, absent socket/key/value, malformed references/storage, oversize files, and symlinked storage. Pre-sign validation failures produce no signing requests; no private-key reads or `op` invocations occur.
- [x] Prefer binary CLI and representative PTY boundaries, supplemented by focused operation/UI tests only where needed; any operation context is implementation discretion, not a new agreed public interface. Regress legacy discovery/providers, passthrough argv, copy helpers, export order/escaping, and concurrency. Follow repository guidelines and run formatting/Clippy/tests with fake-agent isolation.

## Implementation and verification evidence

- Base `d38ea70c001b12641f4021208ec91df668f9b565`; workflow HEAD
  `6f6f8fe`. Reconstructed from files and preserved tickets 01/02 and unrelated
  accumulated edits. Only runtime resolution, consumer gating, tests, and runtime
  documentation were added; no authoring, later slices, requirements changes,
  children, reviews, staging, commits, pushes, or merges.
- Registered exact `local-storage` with provider-owned metadata. References select
  a loaded Ed25519 key by per-ID records or explicit canonical OpenSSH fingerprint.
  Reads reuse ticket 02's config-relative, bounded, private, symlink-refusing strict
  storage validation. Bulk operations share in-flight identity/fetch futures and
  zeroizing results, including errors, without persistent caching or detached work.
  Single fetches and deliberate TUI refetches start fresh operations.
- Binary CLI tests cover mixed/inherited-variable fail-stop launch and absent/existing
  `.env` preservation across reference, key/record/socket, agent/protocol/signature,
  malformed/oversized/duplicate/version/key metadata, permissions, symlink, and
  authentication failures. Copy failure/empty-copy never launches helpers; successful
  copy, exact empty/multiline/Unicode injection, export omission/escaping/order,
  per-user selection, per-ID ambiguity, shared mappings and fresh operations pass.
  Pre-sign failures assert unchanged signing counts. Marker helpers also include
  `op`; runtime uses no private-key-file or external encryption-key lookup.
- Focused isolated operation tests cover ordered individual errors, shared work,
  serialized distinct-record signing, cancellation of active/queued work, subsequent
  usable operations, and deadlines without retry. PTY coverage proves opt-in errors,
  fresh refetch requests, deliberate reveal/hide, cancellation/stale-result rejection,
  continued usability and terminal restoration. Existing legacy discovery/provider,
  argv, concurrency, `op` queue, crypto, persistence and UI regressions pass.
- During development, deadline coverage exposed duplicate signing after a timeout;
  sharing the in-flight future fixed it. The fake delay gate now drains stale permits.
  Clippy caught the shared fixture's selectively unused socket accessor (annotated).
  The first full suite caught a nonportable `/bin/cat` in the clipboard fixture;
  fixed to PATH-based `cat`, and marker helpers use shell builtins. Final full suites
  passed on both toolchains after all changes, without diagnostic retries.
- Final Linux x86_64 checks exited 0: configured Mise formatting/check, locked
  all-target Clippy `-D warnings`, locked full tests (104), Rust 1.88.0 locked full
  tests (104), and Rust 1.88.0 locked build. Logs:
  `/tmp/psst-ticket03-verification/` (`fmt.log`, `fmt-check.log`, `clippy.log`,
  `tests.log`, `tests-1.88.log`, `build-1.88.log`). Commands unset `SSH_AUTH_SOCK`
  and use ticket 01's GCC-wrapper PATH:
  `/nix/store/j1938gsbvbhkyz4kbmvjk56r479mkw3s-gcc-wrapper-16.2.0/bin`.
  GNU timeout budgets: 180 seconds each for formatting/check, Clippy and build;
  240 for configured tests; 300 for Rust 1.88 tests. Fake sockets only; no personal
  agents or live integrations. `git diff --check` passed with a 10-second timeout.
- Independent findings fixed: runtime and PTY fixtures create temporary config
  roots beneath a canonical temporary parent, consistently covering storage,
  config paths, CLI overrides and direct provider calls (including the isolated
  worker). Deliberate storage symlinks remain unresolved; production symlink
  refusal is unchanged. Moved runtime `mod support` below imports (RUST-2) and
  wrapped the bulk-dispatch comment within 80 characters (RUST-1).
- Post-fix Linux x86_64 checks all exited 0: configured Mise formatting/check,
  locked all-target Clippy `-D warnings`, locked full tests (104), Rust 1.88.0
  locked full tests (104), locked build, and `git diff --check`. Both toolchains
  also passed `cargo test --locked --test local_storage_runtime --test terminal`
  (10 tests each, plus isolated worker) with `TMPDIR` pointing through a symlink,
  exercising success, fail-stop, deliberate symlink refusal, cancellation and
  refetch behavior after the import/comment fixes. Native macOS is unavailable;
  this Linux regression does not claim native macOS verification.
- Post-fix logs and complete file inventories:
  `/tmp/psst-ticket03-findings-verification/`. `workflow-inventory.txt` lists all
  31 current changed/untracked files relative to workflow HEAD `6f6f8fe`, including
  `tests/fixtures/local-storage-v1.json` (not 30). `inventory.txt` lists 37 relative
  to base `d38ea70`, including six pre-existing committed workflow/rules files.
  Checks used `env -u SSH_AUTH_SOCK`, fake agents only, and the GCC-wrapper PATH
  above. Finite timeout budgets: 180 seconds for formatting/check, Clippy, build
  and configured affected suites; 240 for configured full tests; 300 for Rust
  1.88 full/affected tests; 10 for diff checking. Logs retain the prior filenames,
  plus `symlink-tmpdir.log`, `symlink-tmpdir-1.88.log` and `diff-check.log`.
  Preserved all accumulated work; stopped before ticket 04 with no requirements
  changes, reviews, children, staging, commits, pushes or merges.
- Remaining RUST-1 finding corrected: rewrapped the entire operation-context
  comment without changing wording or behavior. The final actual line-width scan
  (not formatter inference) checked all 85 introduced comment-only lines across
  19 accumulated Rust files relative to base `d38ea70`, including untracked files:
  zero violations of 80 characters including comment markers, excluding indentation,
  and 100 including indentation. Both before and after formatting scans passed.
- Comment-only correction verification on Linux x86_64 exited 0: configured Mise
  formatting/check, locked all-target Clippy `-D warnings`, configured and Rust
  1.88.0 locked full suites (104 tests each), Rust 1.88.0 locked build, and both
  toolchains' runtime/PTY suites with symlinked `TMPDIR` (10 tests each, plus isolated
  worker). Commands used `env -u SSH_AUTH_SOCK`, fake agents only, and the recorded
  GCC-wrapper PATH above. Finite budgets: 180 seconds for formatting/check,
  Clippy, build and configured affected tests; 240 for configured full tests;
  300 for Rust 1.88 full/affected tests; 10 for `git diff --check` (passed).
  Logs and scan script: `/tmp/psst-ticket03-comment-verification/`, retaining
  the prior check filenames plus `comment-width-scan.log` and `scan.py`.
  Inventories still contain 31 files versus workflow HEAD `6f6f8fe` and 37 versus
  base `d38ea70`. Only this comment and ticket 03 evidence changed in this pass;
  preserved unrelated work and requirements. No new reviews, children, staging,
  commits, pushes or merges; stopped before ticket 04. No verification blocker;
  native macOS and the existing real-agent/crypto/release gates remain unverified.
- Before completion, the ticket remained **in-progress**, criteria unchecked for
  independent review. No implementation/verification blocker was observed.
- Completion rechecked all ten criteria after independent Spec PASS
  (`/tmp/psst-subagent-Ug8NQ8`) and Rules PASS
  (`/tmp/psst-subagent-ll831c`), each with complete 31/31 coverage, exit code 0
  and settled, non-aborted output. No code or test changes were needed.
- Serial completion checks on Linux x86_64 all exited 0: configured Mise
  formatting/check, locked all-target Clippy `-D warnings`, configured and Rust
  1.88.0 locked full tests (104 each), Rust 1.88.0 locked build, and both
  toolchains' runtime/PTY suites with symlinked `TMPDIR` (10 tests each, including
  the isolated-worker launcher). Actual introduced-comment scan against base
  `d38ea70` passed: 85 comment-only lines across 19 Rust files, zero violations.
  `git diff --check` passed. Hashes confirmed all 31 accumulated workflow files
  unchanged by verification; inventory remains 37 files relative to base.
- Completion logs, inventories, hash evidence and scan script:
  `/tmp/psst-ticket03-completion-verification/`. Commands used
  `env -u SSH_AUTH_SOCK`, fake agents only, and ticket 01's recorded GCC-wrapper
  PATH above. Each command had a finite timeout: 180 seconds for formatting/check,
  Clippy, build and configured affected tests; 240 for configured full tests;
  300 for Rust 1.88 full/affected tests; 10 for diff/comment checks. Serial command
  groups were bounded to 600/900 seconds, below the 60-minute limit.
- Marked ticket 03 **done**, checked its criteria, and moved unstarted tickets
  04/06 to **ready-for-agent** because 02/03 are done. Stopped before either;
  the user requires a commit before ticket 04, but this process did not stage or
  commit. Preserved unrelated work; no requirements changes, new reviews,
  children, pushes or merges. No verification blocker. Native-platform,
  real-agent and independent crypto/release gates remain unverified; this is
  ticket 03 completion only, not full-spec or production-readiness approval.
