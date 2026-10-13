# Local storage

Source idea: [Local storage](../ideas/01-local-storage.md)

## Problem Statement

Repository users need local encrypted secrets without a hosted secret manager, while sharing environment-reference configuration and keeping each user's values private. This adds key-lifecycle responsibilities that existing external providers may already handle for some users.

## Solution

Add the `local-storage` provider alongside existing providers. Keep references in the existing configuration and ciphertext in repository-local, per-key storage. Resolve values through an SSH agent and offer initial creation, updates, reference reuse, and deletion through the TUI. The agreed design is confirmed in Q28 of the source idea; it does not authorize implementation.

## User Stories

1. As a repository user, I want encrypted local values resolved through my SSH agent, so that I can inject secrets without a hosted secret store or exporting my private key.
2. As a collaborator, I want shared environment mappings to resolve my own values with explicit identity selection when needed, so that another user's value is never silently substituted.
3. As a TUI user, I want to create, update, reuse, and safely remove encrypted values and references, so that I can manage repository-local secrets without plaintext command arguments.

## Implementation Decisions

### Verified current state

- Configuration discovery selects `PSST_CONFIG`, otherwise the nearest `psst.json` or `.psst.json`, preferring the visible filename at each directory. Configuration is an ordered map of environment names to string `source`/`value` entries. Providers receive a reference and the configuration directory.
- The provider registry owns fetch dispatch and editor metadata. Shared dispatch rejects NUL and returns zeroizing strings; bulk resolution preserves configuration order and individual errors while starting fetches concurrently. There is no operation-local deduplication context or encrypted-storage authoring interface yet.
- Configuration saves already use a cooperative lock, loaded-byte conflict detection, and atomic replacement. The atomic writer syncs the temporary file and directory, uses Unix 0600 files, and can report a directory-sync error after replacement. It does not supply the agreed encrypted-storage validation or symlink protection by itself.
- Fetch failures currently warn and are omitted from injection/export; command launch continues, leaving inherited variables for failed entries. Export omits successful empty values as well. Copy propagates fetch errors. The TUI supports editing references, opt-in fetches, deletion confirmation, asynchronous cancellation/stale-result handling, and short-lived revealed values; its generic paste handling strips NUL rather than rejecting it.
- Existing dependencies do not provide the agreed Ed25519/HKDF-SHA256/AES-256-GCM construction. The project requires Rust 1.88 and supports Linux/macOS, with x86_64 and ARM64 CI targets and musl Linux binaries.

### Agreed provider, identity, and storage contracts

- Persist the exact, case-sensitive source name `local-storage`. Preserve existing configuration discovery and the string-valued environment-reference schema; encrypted storage is separate under `.psst/local-storage/` beside the selected configuration, not beside the caller's working directory.
- Store one JSON file per public key, containing separate records by secret ID. Record identity is `(ID, public key)`; multiple environment mappings may reuse it without duplicating storage. Different users may store different values for the same ID in the shared directory. Use hash-safe key filenames; IDs never become paths. Metadata remains visible, and ciphertext is suitable for committing.
- References are `secret-id` or `secret-id#SHA256:fingerprint`, using the OpenSSH SHA-256 public-key fingerprint. IDs are nonempty, case-sensitive exact strings, excluding `#` and control characters. Conservative additional ID/input limits are delegated.
- An unqualified reference resolves only when exactly one agent key matches a stored value for that ID, not merely an identity elsewhere in storage. Qualified references select that key for that entry. Missing keys or values, unresolved ambiguity, agent rejection, and decryption failures are errors; never fall back to another identity or plaintext.
- Any resolution failure from this provider stops wrapped-command launch and `.env` export, even when other entries succeed or inherited variables exist. Other providers retain their existing best-effort behavior. Copy propagates failures normally; the TUI reports per-entry errors and remains usable. Successful empty values remain valid provider results; existing consumer-specific empty-value behavior is otherwise unchanged.

### Agreed cryptography and agent boundary

- Private-key operations occur exclusively through the standard SSH-agent protocol. Never read private-key files or fetch a separate encryption key using `op`. Target Ed25519 SSH Key items held by 1Password, with 1Password as the required tested integration; allow compatible agents without vendor detection, but advertise only tested integrations.
- Use verified, repeatable deterministic Ed25519 signatures over fixed-format, domain-separated challenges to derive encryption keys with HKDF-SHA256. Encrypt in-process with AES-256-GCM, fresh random salt and nonce on every write, and a versioned envelope authenticating ID, key identity, and version metadata. Verify signatures and check signing repeatability during creation. Validate metadata and sizes before requesting signatures.
- Use maintained Rust cryptographic primitives, not custom primitives or external plugins. Precise v1 JSON fields, challenge encoding, compatible libraries, and hash-safe filenames remain delegated to documented, tested implementation within these contracts; do not treat an example construction as a selected dependency.
- Initially connect only through Unix `SSH_AUTH_SOCK`. Apply existing `PSST_TIMEOUT` semantics (positive seconds, default 30 for missing/invalid values) to each fetch/save agent-work phase, including queued signing time, not human editing time. Serialize signing requests. Timeout or cancellation aborts the phase without automatic retry or fallback; completed file changes follow the persistence contract below.
- Deduplicate identical `(ID,key)` fetches within an operation. Plaintext and derived keys have operation-local lifetimes only, with no persistent cache; TUI refetches start fresh operations. Discard secrets after use and apply best-effort zeroization, without promising safety against a compromised process.

### Agreed validation and persistence

- Bound each per-key file to 4 MiB. Reject malformed data, duplicates, unknown versions, mismatched key metadata, invalid field sizes, and authentication failures. Refuse symlinked storage paths and use private local permissions. Strict validation must precede signing where applicable.
- Per-key saves use locking, loaded-content conflict detection, and atomic replacement; never silently overwrite concurrent same-key edits. Preserve unrelated IDs within the key's file.
- For creating references, persist encrypted storage before configuration. For deletion, persist configuration before removing ciphertext. There is no cross-file transaction promise. Failed reference creation may leave unused ciphertext; failed storage deletion leaves ciphertext. Report partial completion accurately, including failures after replacement, rather than claiming rollback or complete success. This ordering prefers unused encrypted records over dangling references.

### Agreed TUI management

- Creation auto-selects only a sole eligible agent key; otherwise offer a fingerprint chooser. Newly authored references omit the selector only when unambiguous under the resolution rule.
- Edit stored values masked, preserving exact UTF-8 text, including empty and multiline values, and reject NUL rather than silently changing the input. Confirm replacement of an existing `(ID,key)` record, and warn that updates affect all references to it. Reuse existing records without duplicate ciphertext. Never log plaintext or place it in argv.
- When deleting an environment mapping, ask whether to delete its encrypted record too, defaulting to keeping ciphertext. Warn about known shared references in the loaded configuration without claiming repository-wide reference discovery. Delete only an explicitly selected or unambiguously resolved record, never all users' values for an ID; keep ciphertext when identity is unresolved.

## Testing Decisions

### Existing seams and proposed additions

Existing binary-level CLI tests use isolated configurations, fake executables, observable subprocess behavior, and filesystem assertions. PTY tests exercise real TUI keystrokes, rendered output, persistence, and terminal restoration; UI tests also use Ratatui's TestBackend for masking, cancellation, and stale-result behavior. Shared provider dispatch, configuration persistence, and dotenv conversion already have focused tests.

**Proposed testing approach:** reuse CLI and PTY boundaries for end-to-end behavior, backed by one shared deterministic fake SSH agent on a temporary Unix socket. Extend test fixtures to set that socket explicitly and prevent normal tests from inheriting personal-agent access. Retain focused crypto/format, provider-operation, persistence, and UI tests where the agreed lower-level coverage cannot be reliably established through those two high-level boundaries. Any operation context or fault-injection facility needed is a proposed implementation/testing seam, not a new user-approved interface.

### Acceptance coverage

- **CLI/provider boundary:** verify exact reference parsing and ID rules; per-ID zero/one/multiple identity matches; explicit selection with multiple loaded keys; absent keys/records; unsupported keys; agent refusal/disconnection; malformed responses; and successful exact-text decryption. Exercise shared mappings and per-user records, config-relative storage from another caller directory, and unchanged legacy configuration/provider behavior.
- **Fail-stop and consumers:** on each provider-resolution failure class, prove the wrapped command never runs and `.env` is neither created nor replaced, including inherited-variable and mixed-success cases. Verify successful injection/export, empty/multiline handling through existing consumer rules, and copy failure without clipboard-helper execution. Retain other providers' warning/omission policy, passthrough arguments, order, and concurrency regressions.
- **Agent/operation boundary:** count requests to verify deduplication, serialized signing, and fresh TUI refetches without cross-operation caching. Use controllable queued/delayed responses to verify deadline inclusion of queue time, cancellation of active/queued work, no retries/fallback, and usable subsequent operations. Verify no private-key-file access or `op` invocation.
- **Crypto/format boundary:** round-trip empty, multiline, whitespace, Unicode, and boundary-size values; reject NUL. Verify fresh salt/nonce on repeated writes, deterministic verified signatures, and failure on nonrepeatable or invalid signatures. Alter ciphertext, authentication tags, ID/key/version metadata, and envelope sizes to prove rejection. Reject malformed/duplicate/unknown-version records, key/path mismatch, files above 4 MiB, and symlinked storage paths; demonstrate that pre-sign validation failures produce no signing requests. Check documented v1 encoding with stable test fixtures or vectors and selected-library compatibility/security checks.
- **Persistence boundary:** verify private permissions, whole-file atomicity, preservation of other IDs/users, lock contention, and loaded-content conflicts. Inject failures at both stages of creation/deletion and at replacement/sync boundaries; assert ordering, actual surviving artifacts, accurate partial-completion reports, and no silent clobbering. No test should imply cross-file atomic rollback.
- **TUI boundary:** exercise key selection, record creation/reuse/update, selector authoring, replacement/shared-reference warnings, and masked exact-text editing with empty/multiline/paste/NUL cases. Exercise default-keep deletion, explicit deletion, unresolved identity, known shared references, and preservation of other users' records. Cover per-entry errors, cancellation, stale results, fresh refetches, no unintended plaintext rendering/logging/argv, and terminal restoration. Use focused UI tests for exhaustive states and PTY tests for representative complete workflows.

### Release gates

- Require the automated fake-agent, crypto/format, persistence, TUI, and fail-stop regressions above, plus existing formatting, Clippy, tests, and release builds on Linux/macOS x86_64 and ARM64 targets. Verify Rust 1.88 and selected dependencies explicitly; stable CI alone does not establish minimum-version compatibility.
- Require consented, opt-in real 1Password end-to-end checks on Linux and macOS covering creation, subsequent decrypt/update, agent approval/rejection, and repeatability. Normal CI must never contact personal agents or keys. Record tested integrations rather than assuming socket identity proves 1Password custody.
- Require setup, format, recovery/manual-migration, and security documentation explaining the threat boundary and accepted limitations below. Mark the feature experimental until independent crypto/format review establishes the production-readiness gate.
- These are future checks, not completed verification. The source records only a user-authorized toy signature/crypto round-trip and modified-tag rejection; that did not validate a production format, prove 1Password integration, or constitute independent review.

## Out of Scope

- Noninteractive storage-management commands, bare management subcommands, standalone store browser, import/export management, and automated orphan cleanup. Existing runtime `.env` export remains supported; it is not storage export management.
- Automatic migration/rotation, recovery recipients, and multi-recipient shared-secret distribution.
- Key types beyond deterministic Ed25519, private-key-file access, separate encryption keys fetched with `op`, custom cryptographic primitives, and external encryption plugins.
- Agent autodiscovery/startup, automatic retries, Windows agent support, and guaranteed forwarded-agent detection.
- Metadata hiding and replay/rollback protection against valid repository history.
- Changes to other providers' failure policy or replacement of existing configuration discovery/schema.

## Further Notes

- Agent signing authorization grants durable decryption capability: an authorized process can retain signatures/derived keys after the agent locks. Agent-only custody does not keep derived keys out of psst memory. Warn against untrusted agent forwarding and recommend per-operation confirmation; actual approval frequency depends on agent settings.
- Recovery depends on preserving the SSH key in 1Password. Losing it loses decryptability. Manually migrate while the old key works by creating new `(ID,key)` records and updating explicit selectors as needed. Rotation cannot revoke old Git history or captured derived keys.
- Exact editor layout and additional conservative input limits remain delegated alongside the format/library details above. The current provider/editor interfaces will need extension for encrypted authoring and operation-local resolution; their exact shape is not settled by this spec.
