# 01: Verify SSH-agent encrypted round-trips and define v1

**Source:** [Agreed spec](../../../docs/specs/01-local-storage.md), including its linked idea and exclusions.

**What to build:** An independently verifiable path that encrypts exact text through an SSH agent and decrypts the resulting versioned envelope later, without private-key access. Establish the documented v1 contract for subsequent storage and consumer slices; this is not a format-only task or a storage-management CLI.

**Blocked by:** None (can start immediately)

**Status:** done

- [x] Use standard SSH-agent protocol over Unix `SSH_AUTH_SOCK` only, targeting deterministic Ed25519 keys. No private-key-file reads, `op` invocation, vendor detection, agent discovery/startup, external encryption plugins, retries, or fallback. Compatible agents are allowed; advertised integration support requires recorded testing.
- [x] Verify signatures over fixed-format domain-separated challenges, check signing repeatability during creation, derive keys with HKDF-SHA256, and encrypt in-process with AES-256-GCM. Generate fresh random salt and nonce on every write; authenticate ID, key identity, and version metadata.
- [x] Document precise v1 JSON fields, challenge and authenticated-metadata encoding, hash-safe per-key filenames, conservative input limits, and maintained Rust dependency choices. These are delegated implementation decisions, not historical prototype commitments. One per-key JSON file holds separate IDs; identity is `(ID, public key)`, metadata is visible, and IDs never become paths.
- [x] Enforce nonempty, exact case-sensitive IDs excluding `#` and control characters. Validate metadata and sizes before signing where applicable; reject malformed/duplicate data, unknown versions, mismatched key metadata, invalid field sizes, and authentication failures. The per-key format must fit the 4 MiB file bound, including write-size validation.
- [x] Round-trip empty, multiline, whitespace, Unicode, and boundary-size values without trimming; reject NUL. Test changed ciphertext/tags/ID/key/version metadata and sizes, invalid or nonrepeatable signatures, fresh salt/nonce, and stable encoding fixtures or vectors.
- [x] Serialize signing requests. Each fetch/save agent-work phase uses positive `PSST_TIMEOUT` seconds, defaulting to 30 for missing/invalid values; its deadline includes queued signing time but not human editing. Timeout/cancellation aborts active or queued work without retries/fallback, releases resources, and permits subsequent operations.
- [x] Keep derived keys and plaintext operation-local with best-effort zeroization and no persistent cache. Do not log plaintext, put it in argv, or claim safety against a compromised process.
- [x] Verify refusal, disconnection, malformed/oversized agent replies, unsupported keys, delayed/queued work, and cancellation through a controllable deterministic fake Unix-socket agent. This shared fixture is a proposed testing seam, not a public interface; use focused tests where higher-level seams cannot establish the contract.
- [x] Isolate normal CLI/PTY/provider tests from inherited personal-agent access; fake sockets must be explicitly selected. Never contact personal agents in normal tests/CI or run live-agent experiments without consent.
- [x] Record selected-library compatibility/security checks and explicitly verify Rust 1.88 compatibility. Follow repository guidelines and keep formatting, Clippy, and existing tests green. Neither a round-trip nor the source's toy experiment constitutes independent crypto/format review or proven 1Password custody.

## Implementation and verification evidence

Historical implementation evidence before independent review and completion:
status and acceptance checkboxes were unfinished, and tickets 02–07 were blocked.

- Added a bounded Unix SSH-agent client and v1 envelope/key-file encoding without
  provider registration, filesystem persistence, management CLI, or TUI changes.
- The selected [v1 format](../../ARCHITECTURE.md#selected-v1-json-format) records
  fields, exact challenge/AAD/HKDF bytes, filenames, limits, and threat limitations.
  Selected-library/advisory evidence is consolidated in
  [ticket 07](07-experimental-release-readiness.md#selected-dependencysource-checks).
- Fake-socket tests cover exact text and limits, fresh salt/nonce, pre-sign rejection,
  tampering, invalid and valid-but-nonrepeatable signatures, malformed identities/
  replies, refusal/disconnection, serialized/queued work, cancellation, deadlines,
  resource release, and subsequent successful operations. An offline Node vector
  pins byte encodings/derivation and decrypts through the fake agent.
- CLI/PTY subprocess fixtures remove inherited `SSH_AUTH_SOCK`; all verification
  commands also ran with `env -u SSH_AUTH_SOCK`. Fake endpoints were selected
  explicitly. No personal agents, private-key files, or live integrations were used.
- Passed configured Mise `cargo fmt`, `cargo fmt --check`,
  `cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --locked`
  (83 tests). Passed `mise exec -- cargo +1.88.0 test --locked` (same full suite)
  and `mise exec -- cargo +1.88.0 build --locked` on Linux x86_64.
- Normal/build feature-tree check confirms Dalek `hazmat` is absent outside tests.
  `node tests/fixtures/generate-local-storage-v1.mjs` reproduces the fixture exactly.
  `git diff --check` passes.
- Commands used finite GNU `timeout` limits: 30 seconds for formatting/feature-tree,
  180 for Clippy/build, 240 for the configured test suite, 300 for Rust 1.88 tests,
  and 10 for the offline vector. The initial compile lacked `cc`; checks passed
  after adding the existing Nix GCC wrapper to PATH:
  `/nix/store/j1938gsbvbhkyz4kbmvjk56r479mkw3s-gcc-wrapper-16.2.0/bin`.

## Independent-review correction evidence

- Independent ticket 01 Spec review passed with no findings; Rules review found
  one import-order violation. Moved the Agent re-export before the out-of-line
  module declaration. This is declaration ordering only; existing regression
  tests suffice and no behavior or requirements changed.
- After the correction, configured Mise formatting/check and locked all-target
  Clippy passed. The first configured full test run failed when an existing CLI
  cancellation test read an empty PID file after waiting only for its existence
  (`ParseIntError { kind: Empty }`). Left that unrelated test unchanged; a single
  isolated diagnostic rerun passed, followed by all 83 tests passing on both the
  configured toolchain and Rust 1.88.0. The Rust 1.88.0 locked build also passed.
- Checks used the same GCC-wrapper PATH and `env -u SSH_AUTH_SOCK`, with finite
  timeouts: 30 seconds for each formatting command, 180 for Clippy/build, 240
  for configured tests, 300 for Rust 1.88 tests, and 60 for the isolated test.
  Local-storage fake-agent/format, CLI, and PTY regressions passed; no personal
  agents or live integrations were contacted.

## Object-only deserialization correction evidence

- Follow-up labelled Spec blocker: derived Serde deserialization accepted
  positional arrays for `Envelope` and `KeyFile`. Both now require maps before
  delegating to typed field parsers; duplicate/unknown/missing-field rejection
  remains intact. No requirements or unrelated implementation work changed.
  The supplied Rules report passed; no independent reviews were conducted here.
- Added a fake-agent regression for direct envelope arrays, top-level file arrays,
  nested record arrays, and combined arrays. Rejection produces zero signing
  requests; a subsequent named-field fixture decrypt succeeds. Existing duplicate
  field/ID, limits, round-trip, tampering, and stable-vector tests also pass.
- Passed configured Mise formatting/check, locked all-target Clippy with
  `-D warnings`, focused local-storage tests (12), and locked full tests (84).
  Rust 1.88.0 locked full tests (84) and locked build passed. No verification
  failures occurred in this correction run; the earlier CLI PID-file race did
  not recur and remains an unchanged caveat.
- All toolchain commands used `env -u SSH_AUTH_SOCK` and the GCC-wrapper PATH
  recorded above. Finite GNU timeout limits were 180 seconds for each formatting,
  focused-test, Clippy, and build command; 240 for configured full tests; 300 for
  Rust 1.88 tests. Fake sockets only; no personal-agent access. `git diff --check`
  passed after recording this evidence.

## Comment-width correction evidence

- Supplied independent ticket 01 Spec review: PASS, static review of all 22
  accumulated files with no findings. Later slices were not required; this is
  not production approval. Supplied Rules blocker RUST-1 identified seven
  overlong comments. Wrapped those comments without changing their wording,
  behavior, or requirements. No child processes for implementation or independent
  reviews were launched in this correction run.
- Inspected accumulated added Rust comments against workflow HEAD `6f6f8fe`,
  including untracked Rust files: 10 changed Rust files, 52 comment lines,
  zero lines over 80 characters excluding indentation or 100 including it.
  The original workflow base remains `d38ea70`; unrelated work was preserved.
- Passed configured Mise `cargo fmt`, `cargo fmt --check`, locked all-target
  Clippy with `-D warnings`, and locked full tests (84). Rust 1.88.0 locked
  full tests (84) and locked build passed. Both full suites passed on their first
  runs; no test failures were masked by the existing CLI PID-file race caveat.
- Toolchain checks used `env -u SSH_AUTH_SOCK` and the recorded GCC-wrapper PATH.
  Finite GNU timeout limits were 180 seconds for formatting/check, Clippy, and
  build; 240 for configured tests; 300 for Rust 1.88 tests. Fake sockets only;
  no personal agents or live integrations were contacted. `git diff --check`
  passed after recording this evidence. Nothing was staged, committed, pushed,
  or merged.

At that correction boundary, ticket 01 remained in-progress with unchecked
criteria; tickets 02–07 remained blocked. No fresh independent reviews were
conducted in the correction run. The observed CLI test race remains a historical
verification caveat.

## Ticket completion evidence

- Base `d38ea70`; workflow HEAD `6f6f8fe`. Latest independent **Spec PASS**:
  `/tmp/psst-subagent-oGNffq`; **Rules PASS**: `/tmp/psst-subagent-lc7g8R`.
  Each report covers 22/22 accumulated files, with no actionable findings;
  `exit-status` records code 0 and `events.jsonl` records settled, non-aborted
  completion. These are ticket-scoped static implementation/rules reviews,
  not independent crypto/format production approval. No new reviews or children
  were launched during completion.
- Checked all ten criteria against the agent/envelope implementation, focused
  tests and shared fake-agent fixture, subprocess isolation edits, and the
  [selected v1 format](../../ARCHITECTURE.md#selected-v1-json-format). Coverage includes strict object-only
  JSON/duplicates and bounded reads/writes, pre-sign validation, exact text and
  tampering, verified repeatability, queue-inclusive timeout/cancellation and
  subsequent usability, operation-local zeroizing buffers, and documented
  dependency/security limitations. No code/test changes were needed; SHA256
  snapshots confirmed implementation files unchanged throughout verification.
- Serial completion checks all exited 0 on Linux x86_64, configured Rust 1.99.0
  and minimum Rust 1.88.0. Both full suites passed 84 tests on their first runs;
  no failures or diagnostic retries occurred in this run. Logs and the pre-check
  implementation snapshot: `/tmp/psst-ticket01-completion/`.
- Reproduce each toolchain command with this prefix (unset personal-agent access,
  recorded GCC wrapper, finite GNU timeout), substituting the budget and command
  from the table:

  ```sh
  env -u SSH_AUTH_SOCK \
    PATH="/nix/store/j1938gsbvbhkyz4kbmvjk56r479mkw3s-gcc-wrapper-16.2.0/bin:$PATH" \
    timeout <budget>s mise exec -- <command>
  ```

  | Budget | Command | Log |
  | --- | --- | --- |
  | 180 | `cargo fmt` | `fmt.log` |
  | 180 | `cargo fmt --check` | `fmt-check.log` |
  | 180 | `cargo clippy --locked --all-targets -- -D warnings` | `clippy.log` |
  | 240 | `cargo test --locked` | `tests.log` |
  | 300 | `cargo +1.88.0 test --locked` | `tests-1.88.log` |
  | 180 | `cargo +1.88.0 build --locked` | `build-1.88.log` |
  | 30 | `cargo tree --locked -e normal,build,features` | `features.log` |

- Normal/build feature tree contains no Dalek `hazmat`.
  `env -u SSH_AUTH_SOCK timeout 10s node tests/fixtures/generate-local-storage-v1.mjs`
  reproduced the checked-in fixture byte-for-byte (`vector.json`, `vector.log`
  in the log directory). `timeout 10s git diff --check` passed.
  Fake sockets only; no personal agents, private-key files, or live integrations
  were contacted. Nothing was staged, committed, pushed, or merged.
- Ticket 01 is done; unstarted tickets 02/03 are now ready-for-agent, retaining
  their blocker references. Tickets 04–07 remain blocked by their unfinished
  dependencies. No full-spec completion is claimed. Platform/release builds,
  consented live Linux/macOS 1Password checks, and independent crypto/format
  production-readiness review remain outstanding release gates.
