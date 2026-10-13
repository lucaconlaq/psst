# 05: Reuse and update encrypted records through the TUI

**Source:** [Agreed spec](../../../docs/specs/01-local-storage.md), including its linked idea and exclusions.

**What to build:** Users attach additional environment mappings to an existing encrypted record without duplicating ciphertext, and deliberately update that record with clear shared-reference consequences. Extend ticket 04's TUI workflow using the existing safe storage and operation contracts.

**Blocked by:** 04 — Create encrypted secrets through the masked TUI

**Status:** done

- [x] Reuse existing `(ID,key)` records when authoring mappings without duplicate records or ciphertext rewrites. Newly authored selectors remain omitted only when per-ID resolution is unambiguous; multiple loaded keys and users never cause silent value substitution.
- [x] Require explicit confirmation before replacement and warn that an update affects all references to that `(ID,key)`. Known-reference details are limited to the loaded config; do not claim repository-wide discovery.
- [x] Mask editing and preserve exact UTF-8 text, empty/multiline/whitespace/Unicode values, rejecting NUL including paste. Maintain the no-plaintext-logs/argv contract, best-effort zeroization, and fresh-operation fetch semantics; do not persist a plaintext/derived-key cache.
- [x] Replacement produces fresh salt/nonce and keeps all unrelated IDs and other users' records. Existing and newly reused mappings resolve the new value; mappings to another user's same ID remain unchanged.
- [x] Enforce safe per-key locking/conflicts and size/path validation. Retain the documented two-stage ordering whenever a mapping is also created/changed, and report actual partial completion, including post-replacement failures. A declined replacement makes no storage change.
- [x] Test absent/ambiguous identities, agent rejection, timeout including queued signing, cancellation, stale results, lock contention, concurrent edits, and failed saves without retries, fallback, or unintended clobbering. Human editing has no agent deadline.
- [x] Prove ciphertext reuse and shared-update effects with filesystem and runtime assertions. Use focused UI coverage for confirmations/masking/exact text/failure states and representative fake-agent PTY reuse/update workflows, including terminal restoration and UI usability after failures.
- [x] Keep legacy providers/editors, source/reference editing, and formatting/Clippy/tests green. No automatic migration/rotation, recovery recipients, multi-recipient sharing, or new management CLI is introduced.

## Implementation evidence (awaiting independent review)

- Reading/implementation: reconstructed context from the repository with fixed
  original review base `d38ea70`; preserved tickets 01–03 (`671e338`) and 04
  (`b7fdff9`). Marked only 05 `in-progress`; acceptance checkboxes remain unchecked
  for independent review. No children, staging, commits, pushes, merges, requirement
  edits, or ticket 06/07 implementation/status changes.
- Added masked-editor `Ctrl+U` reuse: verify the selected existing record with
  operation-local plaintext, per-key locking and snapshot conflict detection;
  return a per-ID unambiguous/qualified reference without rewriting ciphertext.
  Qualified update references must match the explicitly selected key. Replacement
  confirmation shows known possible references from the loaded configuration only.
  Reuse/config failures and post-update failures report actual surviving changes,
  including effects on existing references.
- Added fake-agent failure checks for reuse and replacement: rejection, absent key,
  missing record, deadline including queued work, cancellation, lock contention,
  concurrent edits, no retries/fallback/clobbering, and mismatched selectors.
  Focused UI tests cover reuse config failures at every write boundary and stale
  results. Two new PTY workflows assert byte-identical reused ciphertext, shared
  runtime update results, fresh salt/nonce, preserved unrelated IDs/other users,
  confirmation/decline, masked exact multiline Unicode text, usable failure states,
  and terminal restoration. Existing exact-text/NUL, stale/cancellation,
  post-replacement persistence and legacy tests remain in the full suites.
- Verification: all toolchain commands ran serially on Linux x86_64 using
  `env -u SSH_AUTH_SOCK timeout <seconds> mise exec -- <command>`, fake agents only,
  PATH prefixed with
  `/nix/store/j1938gsbvbhkyz4kbmvjk56r479mkw3s-gcc-wrapper-16.2.0/bin`.
  Logs: `/tmp/psst-ticket05-verification/`. Configured Rust/Cargo **1.99.0**;
  minimum Rust/Cargo **1.88.0** verified in `toolchain.log` (30-second budget).

  | Command after `mise exec --` | Seconds | Exit | Log |
  | --- | --- | --- | --- |
  | `cargo fmt` | 180 | 0 | `fmt-final.log` |
  | `cargo fmt --check` | 180 | 0 | `fmt-check.log` |
  | `cargo check --locked --all-targets` | 300 | 0 | `check-final.log` |
  | `cargo clippy --locked --all-targets -- -D warnings` | 300 | 0 | `clippy.log` |
  | `cargo test --locked --test local_storage_authoring -- --test-threads=1` | 180 | 0 | `focused-authoring-final.log` |
  | `cargo test --locked --test terminal encrypted_reuse -- --test-threads=1` | 180 | 0 | `focused-pty-final.log` |
  | `cargo test --locked -- --test-threads=1` | 600 | 0 | `test.log` |
  | `cargo +1.88.0 fmt --check` | 180 | 0 | `msrv-fmt-check.log` |
  | `cargo +1.88.0 check --locked --all-targets` | 300 | 0 | `msrv-check.log` |
  | `cargo +1.88.0 clippy --locked --all-targets -- -D warnings` | 300 | 0 | `msrv-clippy.log` |
  | `cargo +1.88.0 test --locked -- --test-threads=1` | 600 | 0 | `msrv-test.log` |
  | `cargo +1.88.0 build --locked` | 300 | 0 | `msrv-build.log` |
  | `cargo build --locked` | 300 | 0 | `build.log` |

- Both full suites passed **127 tests**, including **13 PTY workflows**. Focused
  authoring: **8 passed**; focused reuse PTY: **2 passed**. `timeout 10 git diff
  --check` passed (`diff-check.log`). Changed-path inventories relative to the
  original base and current HEAD, plus nonignored untracked inventory, are stored
  alongside logs.
- Recorded failure: the initial focused authoring run exited **101**
  (`focused-authoring.log`): queued-work assertion incorrectly counted the
  independently active phase's signing request as a retry. Corrected the fixture's
  bound to account for that request, added replacement-failure cases, then the
  focused and both full suites passed. Initial shared-update PTY passed
  (`focused-pty.log`). A later optional log-count helper exited **127** because
  `python3` is unavailable; `awk` established the counts above. No hidden retries
  or repair loop.
- Implementation/verification phase completed; **05 remains in-progress**, not
  done or self-reviewed. No implementation blocker observed. Independent review
  is pending; real-agent, cross-platform and independent crypto/release gates are
  not claimed by this ticket.

## Independent-finding corrections (awaiting re-review)

- Reading/fixes: retained the fixed base `d38ea70` and all accumulated prior work.
  Replaced unbounded confirmation names with a loaded-config-only possible-reference
  count; reserved consequences/controls in a 10-row popup, rendered after the footer.
  Regression coverage uses 100 long environment names at 120×30 and the accepted
  minimum 45×12, and preserves Enter-not-consent and decline-without-writes checks.
- Failures now have a scrollable details panel with persistent PgUp/PgDn hints;
  other keys close it and retain their normal actions. Narrow rendered tests read
  the entire report for reuse and every storage/config persistence failure boundary,
  including shared-update effects, unchanged configuration and uncertain durability.
  Updated the representative PTY assertion and user instructions for bounded counts.
- Rules correction: added the unit side-effect semicolon for the empty identity
  result. Full UI and PTY checks include the no-eligible-key outcome. Existing
  nonblocking base comment widths were left alone. No requirements were rewritten,
  self-review or children launched, later tickets changed, or Git staging/commit/
  push/merge performed. **05 remains in-progress**, acceptance boxes unchecked.
- Verification ran serially, fake agents only, with `SSH_AUTH_SOCK` unset and the
  same recorded GCC PATH prefix above. Logs and base/staged/unstaged/nonignored
  untracked inventories: `/tmp/psst-ticket05-fixes/`. Rust/Cargo versions: configured
  **1.99.0**, minimum **1.88.0** (version logs). All final commands exited **0**:

  | Command after `mise exec --` | Timeout seconds | Final log |
  | --- | --- | --- |
  | `cargo fmt` | 180 | `fmt-final.log` |
  | `cargo fmt --check` | 180 | `configured-fmt-check-final.log` |
  | `cargo check --locked --all-targets` | 300 | `configured-check-final.log` |
  | `cargo clippy --locked --all-targets -- -D warnings` | 300 | `configured-clippy-final.log` |
  | `cargo test --locked -- --test-threads=1` | 600 | `configured-test-final.log` |
  | `cargo build --locked` | 300 | `configured-build-final.log` |
  | `cargo +1.88.0 fmt --check` | 180 | `msrv-fmt-check-final.log` |
  | `cargo +1.88.0 check --locked --all-targets` | 300 | `msrv-check-final.log` |
  | `cargo +1.88.0 clippy --locked --all-targets -- -D warnings` | 300 | `msrv-clippy-final.log` |
  | `cargo +1.88.0 test --locked -- --test-threads=1` | 600 | `msrv-test-final.log` |
  | `cargo +1.88.0 build --locked` | 300 | `msrv-build-final.log` |

- Both full suites passed **127 tests**, including **13 PTY workflows** each.
  Focused `cargo test --locked --lib ui::tests -- --test-threads=1` passed **23**
  (180 seconds, `focused-final2.log`); final full runs include the later prompt fix.
  `timeout 10 git diff --check` passed (`diff-check.log`).
- Recorded intermediate failures, all exit **101**: `focused.log` rejected the
  private Ratatui `line_count` API (replaced with renderer-based scroll clamping);
  `focused-final.log` exposed footer overwrite at 45×12 (popup moved after footer,
  bounded text adjusted); `configured-test.log` passed other suites but failed two
  PTYs because the informational replacement prompt's error flag hid confirmation
  behind the new failure panel (changed prompt to non-error, updated count assertion).
  Final complete checks passed after these corrections; no unresolved verification
  blocker observed. Independent re-review and broader release gates remain pending.

## Responsive chooser correction (awaiting independent re-review)

- Addressed labelled SPEC P2 against fixed base `d38ea70`, preserving accumulated
  staged/unstaged/untracked work. The masked creator reserves usable body space
  on short terminals and shows only the focused field when the complete form
  cannot fit. Tab retains its existing field navigation; all shortcuts and exact
  masked editing remain unchanged. Fingerprints wrap without truncation, and the
  chooser shows the current key position with explicit Enter selection.
- Regression coverage navigates and explicitly chooses eight full-length
  fingerprints at 45×12, 45×16, 55×12, 80×12, 45×30 and 120×30. Each selected
  fingerprint is fully rendered before Enter; multiple identities are not
  auto-selected. Focused fields, fixed masking, exact multiline Unicode draft
  preservation and save/reuse/cancel shortcuts are asserted. Existing narrow
  confirmation and scrollable failure tests remain green. This supersedes the
  earlier insufficient small-terminal chooser evidence for ticket 04; no unmet
  chooser behavior is claimed fixed merely by the older tests.
- Serial verification: fake agents only, `SSH_AUTH_SOCK` unset, recorded GCC PATH
  prefix above. Logs: `/tmp/psst-ticket05-responsive/`, including fixed-base,
  staged, unstaged and nonignored untracked inventories. Configured Rust **1.99.0**
  and minimum Rust **1.88.0** version commands used 30-second timeouts. All final
  commands below used `timeout <seconds> mise exec --` and exited **0**:

  | Commands | Timeout seconds each | Logs |
  | --- | --- | --- |
  | `cargo fmt`; `cargo fmt --check` | 180 | `configured-fmt.log`, `configured-fmt-check.log` |
  | `cargo check --locked --all-targets`; `cargo clippy --locked --all-targets -- -D warnings` | 300 | `configured-check.log`, `configured-clippy.log` |
  | `cargo test --locked -- --test-threads=1` | 600 | `configured-test.log` |
  | `cargo build --locked` | 300 | `configured-build.log` |
  | `cargo +1.88.0 fmt --check` | 180 | `msrv-fmt-check.log` |
  | `cargo +1.88.0 check --locked --all-targets`; `cargo +1.88.0 clippy --locked --all-targets -- -D warnings` | 300 | `msrv-check.log`, `msrv-clippy.log` |
  | `cargo +1.88.0 test --locked -- --test-threads=1` | 600 | `msrv-test.log` |
  | `cargo +1.88.0 build --locked` | 300 | `msrv-build.log` |

- Both full suites passed **128 tests**, including **13 PTY workflows** each.
  Focused UI tests passed **24** (180-second timeout, `focused-final.log`).
  Initial focused run exited **101** because the new test joined nested border
  cells into the wrapped fingerprint assertion; excluded those border cells and
  the final checks passed. `timeout 10 git diff --check` passed.
- **05 remains in-progress**, acceptance boxes unchecked. No verification blocker
  observed; independent re-review remains pending. No requirements, later
  implementation, base comments or prior ticket statuses changed; no self-review,
  children, staging, commit, push or merge.

## Completion after independent review

- Fixed base `d38ea70`; tickets 01–04 remain done and their committed work is
  preserved. Historical pending-review/status notes above describe earlier phases;
  this section records completion of ticket 05 only.
- **Spec PASS:** `/tmp/psst-subagent-UHLotV/`; **Rules PASS for introduced changes:**
  `/tmp/psst-subagent-DCDpbY/`. Each independently covered **38/38 files**, with
  `exit-status` code **0**, `agent_end` without retry and non-aborted `agent_settled`
  in `events.jsonl`. No blockers; nine unchanged baseline comment-width findings
  remain nonblocking and untouched. No new reviews or self-review were performed.
- All eight criteria reconciled with the Spec report's requirement table and fresh
  execution evidence: (1) byte-identical ciphertext reuse and identity-safe selectors;
  (2) explicit consent, decline without writes and loaded-config-only shared warning;
  (3) masked exact text/NUL rejection, operation-local zeroization and fresh fetches;
  (4) fresh salt/nonce, shared update resolution and unrelated ID/user preservation;
  (5) validation, locks/conflicts, storage-before-config and actual partial outcomes;
  (6) identity/agent/queue timeout/cancellation/stale/save failures without retry or
  fallback, with untimed human editing; (7) filesystem/runtime assertions, focused
  UI and fake-agent PTY usability/restoration; (8) green legacy behavior and checks,
  with exclusions unchanged. Acceptance checkboxes are now checked.
- Serial Linux x86_64 verification used `env -u SSH_AUTH_SOCK timeout <seconds>
  mise exec -- <command>`, fake agents only, with PATH prefixed by
  `/nix/store/j1938gsbvbhkyz4kbmvjk56r479mkw3s-gcc-wrapper-16.2.0/bin`.
  Logs: `/tmp/psst-ticket05-completion-2qK7ZD/`; exact commands/exits in `results.log`.
  Configured Rust/Cargo **1.99.0**, minimum Rust/Cargo **1.88.0**, each version
  command limited to 30 seconds. Every verification command exited **0**, no retries.

  | Command after `mise exec --` | Timeout seconds | Log |
  | --- | --- | --- |
  | `cargo fmt` | 180 | `configured-fmt.log` |
  | `cargo fmt --check` | 180 | `configured-fmt-check.log` |
  | `cargo check --locked --all-targets` | 300 | `configured-check.log` |
  | `cargo clippy --locked --all-targets -- -D warnings` | 300 | `configured-clippy.log` |
  | `cargo test --locked --lib ui::tests -- --test-threads=1` | 180 | `focused-ui.log` |
  | `cargo test --locked --test local_storage_authoring -- --test-threads=1` | 180 | `focused-authoring.log` |
  | `cargo test --locked --test terminal encrypted_reuse -- --test-threads=1` | 180 | `focused-pty.log` |
  | `cargo test --locked -- --test-threads=1` | 600 | `configured-test.log` |
  | `cargo build --locked` | 300 | `configured-build.log` |
  | `cargo +1.88.0 fmt --check` | 180 | `msrv-fmt-check.log` |
  | `cargo +1.88.0 check --locked --all-targets` | 300 | `msrv-check.log` |
  | `cargo +1.88.0 clippy --locked --all-targets -- -D warnings` | 300 | `msrv-clippy.log` |
  | `cargo +1.88.0 test --locked -- --test-threads=1` | 600 | `msrv-test.log` |
  | `cargo +1.88.0 build --locked` | 300 | `msrv-build.log` |

- Both full suites: **128 passed**, including **13 PTY workflows** each; focused
  UI/authoring/reuse PTY: **24/8/2 passed**. `timeout 10 git diff --check` passed.
  Before/after code/test manifests (including Cargo manifests/lockfile and Mise
  configuration) compare byte-identically; both SHA-256 manifest digests are
  `4388b447c9d862c016298cf2a14861673ba50351cf05501562d5625cd83240dd`
  (`code-test-before.sha256`, `code-test-after.sha256`, `hash-equality.log`). All
  tracked-file hashes also matched before this ticket-only completion edit.
- **05 done**, no completion blocker. **06 remains ready-for-agent; 07 remains
  blocked until 06 is done.** No code/tests, source requirements, later tickets,
  or unrelated work changed; no children, staging, commit, push or merge. This is
  not full-spec completion or a real-agent, cross-platform, crypto/format production
  approval, or release-gate claim.
