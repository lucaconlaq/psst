# 07: Document and verify experimental release readiness

**Source:** [Agreed spec](../../../docs/specs/01-local-storage.md), including its linked idea and exclusions.

**What to build:** A documented, verifiably experimental feature with accurate compatibility evidence and explicit outstanding production-readiness gates. Documentation and release verification do not authorize contacting personal agents without consent or advertising production readiness prematurely.

**Blocked by:** 03 — Resolve encrypted references safely across runtime consumers; 05 — Reuse and update encrypted records through the TUI; 06 — Delete mappings with optional, identity-safe ciphertext removal

**Status:** in-progress

- [ ] Document setup using an Ed25519 SSH Key item held by 1Password and Unix `SSH_AUTH_SOCK`; explain references, fingerprints, per-ID selection, configuration-relative `.psst/local-storage/`, metadata visibility, commit-safe ciphertext, private local permissions, input/file limits, TUI create/reuse/update/delete, and fail-stop command/export behavior. Only advertise integrations actually verified; allow compatible agents without vendor detection.
- [ ] Publish the implemented v1 format/challenge/authenticated-metadata encoding, hash-safe filenames, dependency rationale and compatibility/security checks, fresh-write randomness, validation rules, operation-local deduplication, serialized signing, and `PSST_TIMEOUT` queue-inclusive phase semantics. Update provider extension documentation to reflect the implemented interfaces without inventing an agreed public API.
- [ ] Explain the threat boundary: agent signing grants durable decryption capability and authorized processes can retain signatures/derived keys after locking; derived keys exist in psst memory. Recommend per-operation confirmation, explain approval frequency depends on settings, and warn against untrusted forwarding. Best-effort zeroization does not protect a compromised process; no guaranteed forwarded-agent detection is promised.
- [ ] Document recovery and manual migration while the old SSH key still works: preserve the key in 1Password, create new `(ID,key)` records, and update explicit selectors as needed. Lost keys lose decryptability; rotation cannot revoke old Git history or captured derived keys. Explain accepted lack of metadata hiding and replay/rollback protection against valid history.
- [ ] Document conflict and partial-failure recovery with actual storage-before-config creation/config-before-storage deletion outcomes, including post-replacement errors, unused ciphertext, and failed ciphertext removal. Never promise cross-file atomic rollback.
- [ ] Preserve exclusions: no noninteractive/bare storage commands, standalone store browser, management import/export, automatic orphan cleanup/migration/rotation, recovery recipients, multi-recipient sharing, other key types, private-key reads, encryption keys fetched via `op`, custom primitives/plugins, agent discovery/startup/retries, Windows support, or changed legacy provider failure policy. Existing runtime `.env` export is not management export.
- [ ] Audit the automated acceptance matrix against the source: fake-agent/crypto-format/provider-consumer/fail-stop/operation/persistence/UI/PTY coverage must be present, including no-sign prevalidation, queue/cancellation/refetch counts, tamper/size/symlink failures, conflicts, partial outcomes, exact text, masking, and terminal restoration. Fill demonstrable coverage gaps without merely duplicating tests from prior tickets. Normal tests/CI never contact personal keys/agents.
- [ ] Run and record repository Mise formatting, formatting check, locked all-targets Clippy with warnings denied, and locked tests. Explicitly verify Rust 1.88 and selected dependency compatibility; stable CI is insufficient MSRV evidence. Require tests/release builds on Linux/macOS x86_64 and ARM64, including Linux musl binaries; keep the existing release checks intact.
- [ ] Provide reproducible consented, opt-in real 1Password end-to-end checks on Linux and macOS for creation, later decrypt/update, approval/rejection, and verified repeatability. Record completed results and actual integration identification; the socket path alone is not proof of 1Password custody. If consent/access/platforms are unavailable, record the unmet release gate rather than mark readiness complete.
- [ ] Explicitly label the feature experimental until independent crypto/format review establishes production readiness. Record review evidence and follow-up findings when available, or clearly leave that production gate outstanding. Neither the source's authorized toy round-trip/tag test nor implementation self-tests are independent review or completed integration evidence. Experimental release still requires the other agreed automated, documentation, and live-integration gates.

## Partial implementation evidence — release blocked

- Original base `d38ea70`; starting HEAD `c01c33e104cb8dda6150ccda38bdc35df1b13d31`,
  branch `implement/01-local-storage-2`. Reconstructed the spec/idea, all seven
  tickets, project instructions and applicable rules. Tickets 01–06 remain done
  and untouched. Started from a clean worktree; only 07 is now **in-progress**.
  Acceptance checkboxes stay unchecked pending independent acceptance; no
  self-review, children, staging, commits, pushes or merges were performed.
- Documentation now covers target 1Password Ed25519 SSH Key setup and explicit
  Unix socket selection (not verified compatibility), commit-safe ciphertext and
  visible metadata, private checkout permissions, exact limits/format/challenge,
  TUI create/reuse/update/delete, fail-stop runtime consumers, threat boundary,
  recovery/manual migration and actual pre/post-replacement partial outcomes.
  Exclusions remain unchanged. Provider extension documentation describes the
  current optional authoring/snapshot interfaces and internal operation context,
  not a promised stable public/plugin API.
- [Readiness record](../../local-storage-readiness.md) contains the audited
  automated acceptance matrix with exact test names, coverage limits, fresh
  selected-library public-source checks, and a reproducible **NOT RUN** consented
  Linux/macOS 1Password protocol. Fresh advisory/source retrieval is bounded,
  not a clean full-graph audit, live integration or independent crypto review.
- Filled one demonstrable matrix gap by extending the existing runtime
  all-consumer failure test with delayed signing and excessive identity count.
  Deadline expiry now proves no wrapped launch, no absent/existing `.env` write,
  no clipboard helper, and exactly four requests for four operations (no retries).
  No production source, dependency, workflow or unrelated test changes.
- Changed paths: `README.md`, `docs/ARCHITECTURE.md`, `docs/local-storage-v1.md`,
  new `docs/local-storage-readiness.md`, this ticket, and
  `tests/local_storage_runtime.rs`.

### Verification

Serial, finite-timeout Mise checks on GNU/Linux x86_64 only; configured Rust/Cargo
**1.99.0**, explicit Rust/Cargo **1.88.0**. Every cargo command used
`env -u SSH_AUTH_SOCK timeout <seconds>s mise exec -- <command>` with PATH prefixed
by `/nix/store/j1938gsbvbhkyz4kbmvjk56r479mkw3s-gcc-wrapper-16.2.0/bin`.
Fake agents only; no personal endpoints, keys or live 1Password operations.
No toolchains/dependencies installed or unsupported platform setups attempted.

Logs: `/tmp/psst-ticket07-verification/`; exact exits in `results.log`, versions
and sole installed target `x86_64-unknown-linux-gnu` in `toolchains.log` (30s).

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

Both full suites: **135 passed**, including **14 PTY workflows**; focused runtime:
**5 passed**. No required toolchain-check failures/retries occurred. Feature tree
confirms selected locked crypto versions and zeroization features, with no
production Dalek hazmat. Before/after manifests for code/tests, Cargo, Mise and
workflows compare unchanged by verification (`code-before.sha256`,
`code-after.sha256`); the authorized test extension preceded those snapshots.
Offline Node fixture regeneration (10s) exited 0 and matched the committed vector
byte-for-byte. SHA-256 of each code manifest:
`c2051ea1d526ad3c6384abee6503133e20cdb609d107b0bc0efb09e8d8c0b01e`.
Diff-whitespace checks caught a documentation trailing space and a ticket trailing
blank line, both removed; final `timeout 10s git diff --check` passed. Relative
Markdown links were checked locally without agents. These are document hygiene
checks, not spec/rules reviews.

### BLOCKERS — do not mark done

- Required macOS x86_64/ARM64 and Linux ARM64 tests/release builds unavailable;
  Linux x86_64/ARM64 musl tests/release binaries unverified. Local GNU/Linux
  builds do not satisfy musl or other-platform gates; CI configuration is not
  evidence of completed runs. Existing release checks remain intact.
- No consent for live Linux/macOS 1Password checks. Creation, later decrypt/update,
  approval/rejection, repeatability and actual integration identification remain
  **NOT RUN** on both platforms. Socket naming and historical toy results do not
  satisfy this gate. No real integration is advertised as verified.
- Independent crypto/format production-readiness review absent; no review evidence
  or follow-up findings can be recorded. Feature remains explicitly experimental,
  and unmet platform/live gates also block experimental release readiness.
- Independent ticket 07 spec/rules reviews remain pending, intentionally not run
  here. This matrix and verification evidence are not review PASS claims.
