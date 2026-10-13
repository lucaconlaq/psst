# 06: Delete mappings with optional, identity-safe ciphertext removal

**Source:** [Agreed spec](../../../docs/specs/01-local-storage.md), including its linked idea and exclusions.

**What to build:** Deleting a `local-storage` environment mapping offers a safe choice to keep or remove its encrypted record, without accidentally deleting another user's value. This slice can use pre-existing encrypted fixtures and does not depend on TUI creation/update.

**Blocked by:** 02 — Safely persist encrypted record changes; 03 — Resolve encrypted references safely across runtime consumers

**Status:** done

- [x] Ask whether to remove the encrypted record too, defaulting to keeping ciphertext. Warn about known shared references in the loaded configuration without claiming repository-wide reference discovery.
- [x] Remove only a record explicitly selected by the reference or unambiguously resolved for that ID. Never delete all users' values for an ID; unresolved identity keeps ciphertext while still allowing safe mapping removal. Selection is not arbitrary and has no other-identity fallback.
- [x] Persist configuration removal before any ciphertext removal. If config persistence fails before replacement, do not remove ciphertext; handle post-replacement errors according to the actual persisted outcome and accurately report partial completion. Failed storage deletion leaves ciphertext, not a rollback claim.
- [x] Preserve unrelated IDs and all other users' records. Use per-key locking, loaded-content conflicts, private permissions, format/size validation, and symlink refusal for optional storage mutation; existing references to a deliberately removed record subsequently fail safely.
- [x] Cover cancel/default-keep/explicit removal, qualified/unqualified references, missing or ambiguous identity, known shared mappings, malformed storage, lock contention, concurrent edits, and symlink paths. No automatic orphan cleanup or unrelated record deletion is allowed.
- [x] Fault-inject both deletion persistence stages and replacement/sync boundaries. Assert order, actual surviving configuration/ciphertext, truthful partial-completion messages, and no silent clobbering or cross-file atomicity promise.
- [x] Use exhaustive focused UI states and representative fake-agent PTY mapping-deletion workflows. Verify other users still resolve their values, a usable UI after failure/cancellation, stale-result safety, no unintended plaintext exposure, and terminal restoration.
- [x] Preserve existing deletion behavior for other providers and repository formatting/Clippy/tests. If identity work contacts the agent, use the agreed phase deadline/cancellation rules with no retries/fallback and no persistent secret cache.

## Historical implementation and correction evidence

- Implemented only 06 atop committed 01–05 (`836e52e`); original fixed review
  base remains `d38ea70`. Status stays `in-progress`; acceptance checkboxes are
  intentionally reserved for independent verification. No self-review, children,
  staging, commits, pushes, merges, requirement edits, or ticket 07 work.
- Default-keep / opt-in selected-record removal; conservative loaded-config
  shared-reference warning. Bounded identity listing performs no signing or
  decryption. Qualified references have no other-identity fallback; ambiguity,
  missing keys/records, rejected listing, or invalid storage keep ciphertext.
- Config-first persistence; uncertain config durability keeps ciphertext even
  after config replacement. Storage errors distinguish unchanged ciphertext from
  completed replacement with uncertain durability. No rollback/orphan cleanup.
- Focused UI tests cover all 25 combinations of successful/injected config and
  storage write, file-sync, replacement, and directory-sync boundaries, including
  ordering, surviving files, and truthful messages. Additional cases cover
  cancellation/default keep/stale results, shared mappings, config/storage locks
  and concurrent edits, and a post-selection symlink swap.
- Fake-agent subprocess tests cover qualified/sole/ambiguous/missing identity,
  agent refusal, malformed storage, symlink, lock, and loaded-snapshot conflict;
  unrelated IDs/other users survive and selection/deletion produces zero signatures.
  PTY workflows cover keep/cancel/remove/ambiguity/missing/malformed/timeout and
  in-flight cancellation, remaining-reference failure, other-user decryption,
  subsequent editing, no plaintext rendering, and terminal restoration.
- Final verification ran serially on Linux x86_64 with personal `SSH_AUTH_SOCK`
  unset, fake agents only, and PATH prefixed by
  `/nix/store/j1938gsbvbhkyz4kbmvjk56r479mkw3s-gcc-wrapper-16.2.0/bin`.
  Every command used `timeout <seconds> mise exec -- <command>`:
  - Configured Rust `1.99.0`: `cargo fmt` / `cargo fmt --check` (180s each),
    `cargo clippy --locked --all-targets -- -D warnings` (300s),
    `cargo test --locked` (600s; 134 top-level tests passed),
    `cargo build --locked --release` (600s): all exit 0.
  - Rust `1.88.0`: `cargo +1.88.0 fmt --check` (180s),
    `cargo +1.88.0 clippy --locked --all-targets -- -D warnings` (300s),
    `cargo +1.88.0 check --locked --all-targets` (300s),
    `cargo +1.88.0 test --locked` (600s; 134 top-level tests passed),
    `cargo +1.88.0 build --locked --release` (600s): all exit 0.
  - Final command logs: `/tmp/psst-ticket06-checks/` (`fmt.log`,
    `fmt-check.log`, `clippy.log`, `test.log`, `build-release.log`,
    `msrv-fmt-check.log`, `msrv-clippy.log`, `msrv-check.log`,
    `msrv-test.log`, `msrv-build-release.log`).
- Earlier verification failures were diagnosed before rerunning: a private
  boundary type in the public test seam; an Esc/q PTY sequence parsed as Alt+q;
  a leftover fake-agent listing permit bypassing the next delayed scenario.
  Corrected visibility, unambiguous Ctrl+C exit, and explicit permit draining.
- Historical counts above are corrected from 144 to 134 for each toolchain:
  summing the `passed` field of every `test result: ok.` line in the original
  `test.log` / `msrv-test.log` gives
  `44+2+18+1+10+5+12+8+4+2+6+5+3+14+0=134`. The 14 PTY tests are
  included, not additional; nested scenarios are not separate top-level tests.
- Independent spec/rules review findings were supplied for correction; new
  independent reviews remain pending. macOS/ARM64/musl and consented real
  1Password/independent crypto review gates were not run or claimed here.

### Independent-finding corrections

- Kept 06 `in-progress`, preserving earlier work and source requirements.
  Bounded the displayed environment name to one clipped line and reserved visible
  consequences, KEEP/REMOVE choice, loaded-config shared-reference warning, and
  controls. Added a 1,000-character-name regression at 45×12, 78×12 (78×11
  popup), and 120×30, including removal/keep and busy states plus cancellation.
- Canonicalized only the UI test fixture's temporary root; deliberate symlink
  mutation tests remain intact. Wrapped the introduced provider doc comment;
  no unchanged base-comment cleanup. Deletion tests reran after this rules fix.
- Fresh verification ran serially on Linux x86_64, personal `SSH_AUTH_SOCK`
  unset, fake agents only, the same GCC PATH prefix above, and
  `TMPDIR=/tmp/psst-ticket06-fixes/tmp-alias` (symlink to `tmp-real`). All final
  commands used `timeout <seconds> mise exec -- <command>` and exited 0:
  - Configured Rust `1.99.0`: `cargo fmt` (180s), `cargo fmt --check` (180s),
    `cargo clippy --locked --all-targets -- -D warnings` (300s),
    `cargo test --locked --lib deletion_` (300s; 4 passed),
    `cargo test --locked` (600s; 135 top-level tests passed),
    `cargo build --locked --release` (600s).
  - Rust `1.88.0`: `cargo +1.88.0 fmt --check` (180s),
    `cargo +1.88.0 clippy --locked --all-targets -- -D warnings` (300s),
    `cargo +1.88.0 check --locked --all-targets` (300s),
    `cargo +1.88.0 test --locked` (600s; 135 top-level tests passed),
    `cargo +1.88.0 build --locked --release` (600s).
  - Logs: `/tmp/psst-ticket06-fixes/` (`fmt.log`, `fmt-check.log`,
    `clippy.log`, `aliased-focused-final.log`, `test.log`, `build-release.log`,
    `msrv-fmt-check.log`, `msrv-clippy.log`, `msrv-check.log`, `msrv-test.log`,
    `msrv-build-release.log`). Both full-suite counts sum to
    `45+2+18+1+10+5+12+8+4+2+6+5+3+14+0=135`, including 14 PTY tests;
    one new top-level regression accounts for the increase.
- Initial aliased focused run exited 101 (3 passed, 1 failed); diagnostic
  long-name rerun also exited 101. The new assertion exposed overlooked block
  padding clipping the name's ellipsis; corrected the display budget, then all
  four deletion tests passed. Failure logs: `aliased-focused.log` and
  `long-name-diagnostic.log` in the fresh log directory. A count-helper attempt
  lacked `python3`; used `awk` to sum the exact logged top-level counts instead.
- No self-review, children, staging, commit, push, merge, or ticket 07 work.
  No remaining correction/verification blocker; independent reviews pending at that phase.

## Completion evidence

- Independent **Spec PASS**: `/tmp/psst-subagent-VopdpL`; independent
  **Rules PASS**: `/tmp/psst-subagent-0VkqOB`. Both reviewed 40/40 files
  against fixed base `d38ea70`, exited 0 with no signal/problem, and emitted
  non-aborted `agent_settled`. No actionable 06 findings. Rules reported nine
  unchanged baseline comment-width findings, nonblocking and outside this slice.
  Prior 01–05 remain done and committed through `836e52e`.
- All eight criteria reconciled using the independent acceptance trace and fresh
  checks, not a completion self-review: (1) default keep/shared warning;
  (2) qualified/sole-record selection and unresolved-identity keep;
  (3) config-first persistence and truthful partial outcomes;
  (4) per-key preservation/locking/conflicts/validation/permissions/symlink refusal;
  (5) cancel/identity/malformed/lock/conflict/symlink regressions;
  (6) 25 config/storage persistence-boundary combinations;
  (7) focused UI consent/long-name/stale-result states and fake-agent PTY
  workflows, other-user resolution, masking, usability, and restoration;
  (8) legacy deletion and bounded cancellable agent listing without retries,
  fallback, signing, or persistent cache. Detailed static evidence is in the
  Spec PASS report; execution evidence follows.
- Fresh checks ran serially on Linux x86_64 with `SSH_AUTH_SOCK` unset, fake
  agents only, PATH prefixed by
  `/nix/store/j1938gsbvbhkyz4kbmvjk56r479mkw3s-gcc-wrapper-16.2.0/bin`, and
  `TMPDIR=/tmp/psst-ticket06-completion-3eu9Ei/tmp-alias` pointing to `tmp-real`.
  Every command used `timeout <seconds> mise exec -- <command>`; all exited 0:
  - Configured Rust `1.99.0`: `cargo fmt` (180s), `cargo fmt --check` (180s),
    `cargo clippy --locked --all-targets -- -D warnings` (300s),
    `cargo test --locked --lib deletion_` (300s; 4 passed),
    `cargo test --locked` (600s; 135 passed),
    `cargo build --locked --release` (600s).
  - Rust `1.88.0`: `cargo +1.88.0 fmt --check` (180s),
    `cargo +1.88.0 clippy --locked --all-targets -- -D warnings` (300s),
    `cargo +1.88.0 check --locked --all-targets` (300s),
    `cargo +1.88.0 test --locked --lib deletion_` (300s; 4 passed),
    `cargo +1.88.0 test --locked` (600s; 135 passed),
    `cargo +1.88.0 build --locked --release` (600s).
  - Exact commands, exit codes, toolchain versions, environment, and per-command
    logs: `/tmp/psst-ticket06-completion-3eu9Ei/` (`checks.txt`,
    `toolchains.log`, `environment.txt`, and named `.log` files).
    Each full suite sums the logged top-level `passed` fields:
    `45+2+18+1+10+5+12+8+4+2+6+5+3+14+0=135`. The 14 PTY tests are included;
    focused reruns and nested scenarios are not additional full-suite tests.
- Before/after SHA-256 manifests cover every file under `src/` and `tests/`,
  plus `Cargo.toml`, `Cargo.lock`, and `mise.toml`; `cmp` passed unchanged.
  Both `code-tests-before.sha256` and `code-tests-after.sha256` in the log
  directory hash to
  `71f0f1a06d552fae822f17b00e081444fc27df9e88421e97178b37fab9bc76ee`.
  No code/test changes, failed completion checks, retries, new reviews, children,
  staging, commits, pushes, merges, requirement changes, or 07 implementation.
- 06 is done; all listed blockers of unstarted 07 are now done, so 07 becomes
  `ready-for-agent` with blocker references retained. External-platform/musl,
  consented live 1Password, and independent crypto/format production gates remain
  unclaimed and outstanding; these PASS reports are not full-spec release approval.
