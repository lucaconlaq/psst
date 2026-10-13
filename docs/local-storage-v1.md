# Experimental local-storage v1 contract

The agent/envelope library introduced in ticket 01 now supports the registered
provider, config-relative persistence, runtime resolution, and masked TUI creation,
reuse, and updates.
There is no storage-management CLI. No real integration is advertised as
tested: only the deterministic fake Unix agent has been exercised. Independent
crypto/format review and consented Linux/macOS 1Password checks remain outstanding.

## Runtime use (experimental)

The case-sensitive provider source is `local-storage`. Existing configuration
entries use `"source": "local-storage"` and `"value": "secret-id"` or
`"value": "secret-id#SHA256:fingerprint"`. Ciphertext is read from
`.psst/local-storage/` beside the selected configuration (including `PSST_CONFIG`),
not the caller's directory. Runtime use requires an eligible loaded SSH-agent key
and a v1 record. There is no storage-management CLI.

## Masked TUI creation (experimental)

Press `a`, select `local-storage` in the source field, then `Ctrl+N` to create an
encrypted value. `Tab` cycles environment name, exact ID, masked value, and SSH
fingerprints. A sole eligible Ed25519 key is selected automatically; with multiple
keys, use up/down and `Enter` in the fingerprint field to select explicitly.
`Enter` in the value field inserts a newline; bracketed paste preserves exact
UTF-8, whitespace, and empty values. NUL rejects the entire paste. The value stays
masked, including when using the ordinary editor's reveal/preview shortcuts.
`Ctrl+S` saves; existing `(ID,key)` records require `y` confirmation with a warning
that all references to that record are affected. The bounded count of known
possible references covers only the loaded configuration, not repository-wide
discovery. `n` retains
the draft unchanged. To update, open an existing entry and press `Ctrl+N`, then
enter the replacement exact text (the editor starts empty, not with cached plaintext).
A qualified reference must match the selected fingerprint.

In the masked editor, `Ctrl+U` attaches the environment mapping to the selected
existing `(ID,key)` record. It verifies decryption but discards the plaintext and
never rewrites ciphertext; the masked value draft is ignored. Reuse requires an
existing record and the same agent deadline, path validation, per-key lock, and
loaded-byte conflict checks as saving.

Failures open a details panel: `PgUp`/`PgDn` scroll the complete report, including
partial completion and durability warnings. Other keys close the panel and perform
their normal action; `Ctrl+C` still quits safely.

Human editing has no agent deadline. Save rechecks loaded identities and uses one
`PSST_TIMEOUT` agent-work budget, including queued requests, without retry or
fallback. Authored references omit the selector only when the selected key is the
sole loaded key with this ID; otherwise they include its OpenSSH fingerprint.

Ciphertext is persisted before configuration. Storage failure leaves configuration
unchanged; config failure may leave unused ciphertext. After replacement, existing
references can already resolve the new value even if configuration saving fails.
Reuse saves only configuration and reports its actual replacement/durability state. Post-replacement sync
failures report changed files with uncertain durability, not rollback. `Esc`
cancels unfinished work and discards the zeroizing draft; already completed writes
are not undone. Late completed storage results never save an old draft's reference
into a new draft. Reopen the configuration after external edits; retained unused
records are not automatically cleaned up. Encrypted deletion remains later ticket
scope.

An unqualified reference requires exactly one loaded eligible key with that ID;
other IDs do not count. A selector restricts resolution to that fingerprint.
Resolution errors stop wrapped-command launch and `.env` export, without replacing
an existing `.env`. Other providers retain warning/omission behavior. Empty values
are injected, omitted from export, and rejected by copy as before. Successful
bulk reads share only an operation-local `(ID,key)` fetch; single reads and TUI
refetches always start fresh. Returned and cached plaintext uses best-effort
zeroization. Existing deliberate TUI reveals remain short-lived.

## Identity and limits

Identity is `(ID, public key)`. IDs are exact, case-sensitive UTF-8, 1–256 bytes,
without `#` or Unicode control characters (`char::is_control`). No normalization
or trimming occurs. IDs never become paths, even if they contain slashes or dots.
Secret text is exact UTF-8, 0–1,048,576 bytes, including whitespace and newlines,
excluding NUL. These byte limits apply before requesting signatures.

Only Ed25519 is eligible. A canonical SSH public-key blob is:

```text
string("ssh-ed25519") || string(raw 32-byte Ed25519 public key)
```

Here and below, `u32` is unsigned, four-byte big-endian; `string(b)` is
`u32(byte_length(b)) || b`. The blob has exactly 51 bytes. Invalid encodings,
trailing bytes, invalid points, and weak public keys are rejected. An OpenSSH
fingerprint is `SHA256:` plus standard unpadded base64 of SHA256(the entire blob).
The filename is the lowercase 64-character hex of that same hash plus `.json`.
No ID, agent comment, or fingerprint text is used as a filesystem path.

Storage is read and written in `.psst/local-storage/` beside the selected
configuration. Saves create missing configuration-directory ancestors before
persisting ciphertext, without writing configuration first. Storage directories
use Unix 0700 permissions and files use 0600; existing storage with nonprivate
permissions and symlinked storage paths or ancestors are refused. Per-key saves
use cooperative locking, loaded-content conflict detection, and synced atomic
replacement. Files contain at most 1,024 records and at most 4,194,304 encoded
JSON bytes, on reads **and** writes. Reads are bounded before parsing;
serialization stops at the byte bound rather than allocating an entire oversized
output first.

## JSON

A per-key file has exactly these required fields:

| Field | Type / meaning |
| --- | --- |
| `version` | Integer `1`; selects this entire construction |
| `public_key` | Canonical padded standard base64 of the 51-byte SSH blob |
| `fingerprint` | Exact OpenSSH SHA256 fingerprint of `public_key` |
| `records` | Array of envelopes; distinct IDs, empty array allowed |

Each envelope has exactly these required fields:

| Field | Type / meaning |
| --- | --- |
| `version` | Integer `1` |
| `id` | Exact ID string |
| `public_key` | Same canonical base64 SSH blob as the file |
| `salt` | Canonical padded standard base64 of 32 random bytes |
| `nonce` | Canonical padded standard base64 of 12 random bytes |
| `ciphertext` | Canonical padded standard base64 of ciphertext followed by the 16-byte GCM tag |

Ciphertext+tag is 16–1,048,592 decoded bytes. Unknown/missing/duplicate fields,
duplicate IDs, unknown versions, malformed JSON, noncanonical base64, invalid
binary sizes, mismatched keys/fingerprints/filename, or oversized files are errors.
JSON property order and whitespace are not cryptographic inputs. The writer emits
compact JSON; the parser accepts bounded JSON whitespace. The whole document is
validated, including every record, before it is handed to later signing/persistence
work. The envelope decrypt API also requires the expected key and ID and validates
those and all envelope fields before signing. File headers are checked for exact
consistency; version/key/ID are cryptographically bound inside every record.
Neither header nor ciphertext provides authentication of the set/order of records:
removal, rearrangement, and replay of previously valid records are not prevented.

## Exact cryptographic encoding

`||` denotes byte concatenation. Quoted domain strings below are ASCII, with an
actual zero byte at `\0`. No implicit terminators, delimiters, or encoding of JSON
are added. The nonce and salt have fixed sizes and therefore no length prefixes.

```text
AAD = "psst/local-storage/aad\0"
      || u32(1)
      || string(SSH public-key blob)
      || string(ID UTF-8 bytes)
      || salt[32]
      || nonce[12]

challenge = "psst/local-storage/sign\0" || AAD

signature = verified raw 64-byte Ed25519 signature over challenge
info = "psst/local-storage/hkdf\0" || AAD
key = HKDF-SHA256(salt=salt[32], IKM=signature, info=info, L=32)

ciphertext_and_tag = AES-256-GCM(key, nonce[12], exact UTF-8 plaintext, AAD)
```

Creation obtains two signatures over the identical challenge, strictly verifies
both against the selected public key, and requires byte-for-byte repeatability.
Decryption obtains and strictly verifies one signature. Signatures use ordinary
Ed25519, not Ed25519ph. Every write obtains a fresh salt and nonce from the OS CSPRNG;
randomness failure is an error. Unsupported signatures, authentication failure,
non-UTF-8 decrypted text, and NUL are errors with no retry/fallback.

### Stable interoperability fixture

[Fixture JSON](../tests/fixtures/local-storage-v1.json) decrypts to `" \n🔑\t"`.
It uses the **public test seed** `[7;32]`, ID `fixture`, salt `[1;32]`, nonce `[2;12]`.
The fixed inputs are fixtures only, never production randomness. Its filename is
`cff7d2bf46744594be2dc71b73a668396c2dfdf6e3d5514919204a41ba3889c1.json`.

[Offline generator](../tests/fixtures/generate-local-storage-v1.mjs) reproduces
it using Node's built-in Ed25519/HKDF/AES-GCM, without agents or key-file access:
`node tests/fixtures/generate-local-storage-v1.mjs` (JSON on stdout, intermediate
vectors on stderr). Normal Rust tests require neither Node nor any real agent.
Focused Rust tests pin exact AAD/challenge bytes, verify this signature, compare
the derived key, and decrypt the fixture through the fake agent:

```text
signature = 8336f0db0be5dbe97128aa502b26b5a9fd39bb6c51a81b3b1a32ecf44148f9909
            274678f82a2afd55bc4a2817fcf362d6cd9d3e939e980aef7fbd666a069ee04
key       = 81e0bb0d85aadb08630da834df74405d81b05bde51fca33ff7cea2419798bf0d
```

The fake agent also models two *valid but different* Ed25519 signatures using
Dalek's test-only `hazmat` API on this public test seed. Production builds do not
enable that feature and never sign locally. This is fault injection, not a new
cryptographic primitive or an alternative production signing path.

## Agent protocol and lifetime

Only Unix `SSH_AUTH_SOCK` is selected by `Agent::from_env`; tests explicitly
construct `Agent::at(fake_socket)`. There is no private-key-file access, `op`,
vendor detection, discovery/startup, forwarding detection, plugin, or fallback.
The standard protocol uses identity request/answer `11/12` and sign request/answer
`13/14`, with sign flags `u32(0)`. Unsupported identity types are ignored; malformed
Ed25519 identities and duplicate eligible identities are errors. Comments are
ignored. Response frames are 1–262,144 bytes and lists have at most 1,024 identities;
frames and nested strings must be complete with no trailing data.

A process-wide asynchronous mutex serializes individual identity exchanges and
signing/crypto sections, even across different endpoint handles. Both creation
signatures, their repeatability check, key derivation, and encryption share one
guard; decryption signing, verification, key derivation, and decryption also share
one guard. Identity listing acquires a separate guard, so other work may interleave
between listing and signing. The encompassing fetch/save agent-work deadline is
distinct from these mutex sections: it uses positive `PSST_TIMEOUT` seconds,
default 30 for missing/invalid/zero values, and includes queueing, connecting,
identity listing, signing, verification and encryption/decryption. Human editing
is outside this API and deadline. Cancellation is dropping the future. Cancellation
or timeout drops its queue waiter/guard and its operation-owned socket; subsequent operations
open a new socket. There are no detached background signing tasks or retries.
Closing the socket cannot promise to dismiss an agent's own approval UI.

No persistent plaintext, signature, or derived-key cache exists. Returned plaintext
is a zeroizing string owned by the caller's operation. Internal plaintext/signature
buffers and derived key arrays use zeroizing guards, including error/cancellation
paths. Encryption reserves tag capacity to avoid reallocating an unwiped plaintext
buffer. Enabled AES/GHASH/POLYVAL zeroization features cover available library key
state cleanup. This is best effort: stack copies, HKDF internals, compiler behavior,
OS memory, or a compromised process are not guaranteed protected. Callers retain
responsibility for their input text's lifetime. Diagnostics never include plaintext,
and the API does not put plaintext into argv.

Agent signing authorization grants **durable decryption capability**: an authorized
process can retain signatures/keys even after the agent locks. Avoid untrusted agent
forwarding; prefer per-operation confirmation (actual prompts depend on agent
settings). Metadata is visible. Losing the SSH key loses decryptability; manual
migration must occur while the old key works. Rotation does not revoke old Git
history or captured keys. No replay/rollback protection or metadata hiding is claimed.

## Selected dependencies and security checks

Selected established stable RustCrypto/Dalek releases are locked in Cargo.lock:
`aes-gcm 0.10.3`, `ed25519-dalek 2.2.0`, `curve25519-dalek 4.1.3`, `hkdf 0.12.4`,
`sha2 0.10.9`, and `rand_core 0.6.4`/`getrandom 0.2.17`. They provide maintained
primitives rather than a custom construction of AES, Ed25519, or HKDF. The versions
are chosen for the compatible established API family, not claimed to be the newest
major versions. Direct `aes 0.8.4`, `ghash 0.5.1`, and `polyval 0.6.2` dependencies
only enable cleanup features on AES-GCM's existing dependency graph. Existing
`data-encoding` supplies canonical base64/hex. No SSH/encryption executable is used.

Library-source/API and published-security checks during implementation:

- [AES-GCM security notes](https://docs.rs/aes-gcm/0.10.3/aes_gcm/) describe the NCC
  audit and constant-time-multiplication platform caveat. That library audit is
  **not** a review of this composition/format. x86_64/ARM64 are the project targets;
  unsupported variable-time-multiply processors are not newly supported.
- [RUSTSEC-2023-0096](https://rustsec.org/advisories/RUSTSEC-2023-0096.html),
  unauthenticated plaintext exposure on failed in-place decryption, is patched in
  the selected AES-GCM 0.10.3. This caller returns no plaintext on any error.
- [RUSTSEC-2022-0093](https://rustsec.org/advisories/RUSTSEC-2022-0093.html) is patched
  in Dalek >=2; production uses only validated public keys and `verify_strict`,
  with weak-key rejection and no hazmat/signing API.
- [RUSTSEC-2024-0344](https://rustsec.org/advisories/RUSTSEC-2024-0344.html) is patched
  in selected curve25519-dalek 4.1.3.
- [RUSTSEC-2021-0100](https://rustsec.org/advisories/RUSTSEC-2021-0100.html) affects
  SHA2 0.9.7, not selected 0.10.9. Rand-core advisories
  [2019-0035](https://rustsec.org/advisories/RUSTSEC-2019-0035.html) and
  [2021-0023](https://rustsec.org/advisories/RUSTSEC-2021-0023.html) are patched before
  selected 0.6.4; production uses fallible `OsRng`, not a seeded PRNG.
- Declared MSRVs are 1.56 for AES-GCM/AES/GHASH and 1.81 for Ed25519-Dalek. Actual
  Rust 1.88 locked build/test evidence is recorded in ticket 01; declarations and
  a newer compiler alone are not compatibility evidence.

These are bounded selected-library checks against published advisories, not a
complete dependency audit or independent crypto/format review. Fake-agent tests
and the offline interoperability vector do not prove 1Password custody or actual
approval/repeatability behavior. Release gates from the source spec remain in force.
