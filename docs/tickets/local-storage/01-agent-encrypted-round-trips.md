# 01: Verify SSH-agent encrypted round-trips and define v1

**Source:** [Agreed spec](../../../docs/specs/01-local-storage.md), including its linked idea and exclusions.

**What to build:** An independently verifiable path that encrypts exact text through an SSH agent and decrypts the resulting versioned envelope later, without private-key access. Establish the documented v1 contract for subsequent storage and consumer slices; this is not a format-only task or a storage-management CLI.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] Use standard SSH-agent protocol over Unix `SSH_AUTH_SOCK` only, targeting deterministic Ed25519 keys. No private-key-file reads, `op` invocation, vendor detection, agent discovery/startup, external encryption plugins, retries, or fallback. Compatible agents are allowed; advertised integration support requires recorded testing.
- [ ] Verify signatures over fixed-format domain-separated challenges, check signing repeatability during creation, derive keys with HKDF-SHA256, and encrypt in-process with AES-256-GCM. Generate fresh random salt and nonce on every write; authenticate ID, key identity, and version metadata.
- [ ] Document precise v1 JSON fields, challenge and authenticated-metadata encoding, hash-safe per-key filenames, conservative input limits, and maintained Rust dependency choices. These are delegated implementation decisions, not historical prototype commitments. One per-key JSON file holds separate IDs; identity is `(ID, public key)`, metadata is visible, and IDs never become paths.
- [ ] Enforce nonempty, exact case-sensitive IDs excluding `#` and control characters. Validate metadata and sizes before signing where applicable; reject malformed/duplicate data, unknown versions, mismatched key metadata, invalid field sizes, and authentication failures. The per-key format must fit the 4 MiB file bound, including write-size validation.
- [ ] Round-trip empty, multiline, whitespace, Unicode, and boundary-size values without trimming; reject NUL. Test changed ciphertext/tags/ID/key/version metadata and sizes, invalid or nonrepeatable signatures, fresh salt/nonce, and stable encoding fixtures or vectors.
- [ ] Serialize signing requests. Each fetch/save agent-work phase uses positive `PSST_TIMEOUT` seconds, defaulting to 30 for missing/invalid values; its deadline includes queued signing time but not human editing. Timeout/cancellation aborts active or queued work without retries/fallback, releases resources, and permits subsequent operations.
- [ ] Keep derived keys and plaintext operation-local with best-effort zeroization and no persistent cache. Do not log plaintext, put it in argv, or claim safety against a compromised process.
- [ ] Verify refusal, disconnection, malformed/oversized agent replies, unsupported keys, delayed/queued work, and cancellation through a controllable deterministic fake Unix-socket agent. This shared fixture is a proposed testing seam, not a public interface; use focused tests where higher-level seams cannot establish the contract.
- [ ] Isolate normal CLI/PTY/provider tests from inherited personal-agent access; fake sockets must be explicitly selected. Never contact personal agents in normal tests/CI or run live-agent experiments without consent.
- [ ] Record selected-library compatibility/security checks and explicitly verify Rust 1.88 compatibility. Follow repository guidelines and keep formatting, Clippy, and existing tests green. Neither a round-trip nor the source's toy experiment constitutes independent crypto/format review or proven 1Password custody.
