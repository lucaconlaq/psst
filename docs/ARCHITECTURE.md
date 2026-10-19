# Architecture

## Purpose

psst manages local configuration entries and resolves secrets for interactive use
or injection into a command's environment. Behavioral requirements belong to the
[specs](specs/); this document describes the implemented modules, interfaces, and selected format.

## Entry Points

`src/main.rs` starts the interactive console, wraps commands, or handles clipboard
copying and `.env` export. `src/cli.rs` parses arguments. There is no separate
storage-management CLI or standalone store browser.

## Technology used

Rust with Tokio for asynchronous work, Ratatui and Crossterm for the terminal UI,
and Serde for JSON configuration.

Local-storage uses the locked RustCrypto/Dalek graph: `aes-gcm 0.10.3`,
`ed25519-dalek 2.2.0`, `curve25519-dalek 4.1.3`, `hkdf 0.12.4`, `sha2 0.10.9`,
and `rand_core 0.6.4`/`getrandom 0.2.17`. These are established compatible API
families, not a claim to use the newest major releases. Direct `aes 0.8.4`,
`ghash 0.5.1`, and `polyval 0.6.2` dependencies enable cleanup features on
AES-GCM's existing graph. `data-encoding` supplies canonical base64 and hex.
Production builds do not enable Dalek hazmat or sign locally. Selected-library
checks and compatibility evidence remain in the retired tickets' Git history;
they do not constitute a current full dependency-security audit.

## Main Modules

- `config`: configuration discovery, validation, and persistence.
- `providers`: provider registry and secret resolution.
- `browser`: provider-independent reference browsing interfaces.
- `ui`: interactive editing and presentation.
- `process`: bounded subprocess execution and cancellation cleanup.
- `operations`: clipboard copying and dotenv export.
- `push`: bounded, exact-text managed-file snapshots, target grouping, batch
  approval, encrypted updates, and grouped manual configuration persistence.

## Module Boundaries / Seams

Providers own fetching, editor metadata, and optional reference browsing. Each
implementation lives in `src/providers/` and implements `Provider`; registration
is centralized in `src/providers/mod.rs`. Registry order determines the editor's
source-selector order.

The UI consumes shared provider and browser interfaces rather than selecting
provider-specific behavior. Browser rows expose reference metadata, not retrieved
secrets. Local-storage browsing scans validated records across stored public keys
without contacting the SSH agent. Encrypted authoring is launched from that browser.

### Extending providers

Implement `Provider` and register it in `PROVIDERS`. Its exact `id` is persisted
as `source`; `reference_label` and `sensitive_reference` own editor metadata.
`browser` optionally returns a shared `Level`. `fetch` receives the stored value
and selected configuration directory and returns `FetchFuture`, a boxed future
yielding a zeroizing string. Shared `providers::fetch` rejects NUL.

`Provider::author` optionally exposes `Author`. It lists public `Identity`
fingerprints with opaque provider-owned tokens; `prepare` returns a
conflict-checked `PreparedSave` snapshot. The snapshot reports replacement and
shared-reference metadata; `save` persists ciphertext and returns a reference.
Existing references are selected through browsing, without decryption or
rewriting ciphertext; there is no separate reuse API.

`prepare_deletion` selects a `PreparedDelete` snapshot without plaintext for
mapping deletion; the UI saves configuration before calling `remove`.
`prepare_browser_deletion` selects an existing qualified reference directly from
validated storage without agent access and returns the same snapshot interface;
its `remove` keeps configuration unchanged. `WriteBoundary` is the existing
persistence fault-injection seam, not a user-facing plugin API. These are current
interfaces, not an agreed stable extension API.

Bulk resolution creates one local-storage `Operation` for shared identity listing
and `(ID,key)` fetch deduplication. Single fetches and TUI refetches start fresh
operations. This context is internal, not an added provider trait contract.

### SSH-agent boundary

Unix `PSST_SSH_AUTH_SOCK` selects the runtime endpoint when set; otherwise
`SSH_AUTH_SOCK` is used. An empty selected value or missing fallback is an error.
Connection/agent failures do not fall back to another endpoint. Tests explicitly
construct `Agent::at(fake_socket)` or isolate both environment variables.
psst does not read SSH's `IdentityAgent` or private keys, invoke `op` for local
storage, detect vendors or forwarding, discover/start agents, or retry/fall back.

The standard protocol uses identity request/answer `11/12` and sign request/answer
`13/14`, with sign flags `u32(0)`. Unsupported identity types are ignored;
malformed Ed25519 identities and duplicate eligible identities are errors.
Comments are ignored. Response frames are 1–262,144 bytes; lists contain at most
1,024 identities. Frames and nested strings must be complete, with no trailing data.

A process-wide asynchronous mutex serializes individual identity exchanges and
signing/crypto sections across endpoint handles. Both creation signatures,
repeatability validation, derivation, and encryption share one guard; decryption
signing, verification, derivation, and decryption share another section. Identity
listing takes a separate guard, so work can interleave between listing and signing.

The encompassing fetch/save phase uses positive `PSST_TIMEOUT` seconds, default
30 for missing, invalid, or zero values. It includes queueing, connecting,
identity listing, signing, verification, and encryption/decryption, not human
editing. Dropping a future cancels its queue waiter/guard and owned socket;
subsequent operations open a new socket. There are no detached signing tasks.
Closing a socket cannot promise to dismiss the agent's approval UI.

## Data Flow

Load configuration → resolve entries through providers → inject successful
results into a command, export them, or present them in the console. Resolution
preserves configuration order and individual failures; fetches run concurrently
except for queued 1Password reads and serialized local-storage agent work.
Local-storage failures stop wrapped-command launch and `.env` export; other
providers retain warning/omission behavior. Copy propagates errors. Successful
empty values are injected, omitted from export, and rejected by copy.

### Managed-file push

Push owns local-file plaintext in zeroizing memory, without pull's disk staging.
It resolves local-storage references to existing `(ID, public key)` targets before
coalescing identical inputs or rejecting conflicts, then compares raw decrypted
values. One operation pins its SSH-agent endpoint; it does not persist plaintext
or agent results beyond that operation. Manual literals are compared directly.

After batch approval, encrypted targets and eligible `op` items are saved
independently before one manual configuration save. Each encrypted update reloads latest configuration and
storage, authenticates the current record, and retains configuration/per-key locks,
loaded-byte checks, and atomic replacement. Inputs and mappings are revalidated
before work, after signing with refreshed eligible identities, and at final
replacement. Latest source values may be overwritten, but removed, retargeted,
ambiguous, unsafe, or finally conflicting targets fail. This path cannot create
records and does not reuse creation's stale authoring snapshot. Cancellation
releases owned work while preserving completed writes; postreplacement failures
report uncertain durability. TUI authoring remains separate and unchanged.

`providers::op::push` owns strict Secure Note JSON eligibility and canonical
vault/item/section/field resolution. Its closed schema rejects attachments,
unknown properties, unsupported fields, and all other categories. In particular,
Login JSON cannot establish passkey absence. Push groups resolved fields before
raw comparison, then combines approved fields from a freshly loaded item while
preserving recognized metadata. Optional string `additional_information` is
round-tripped as a CLI-generated display summary; the CLI regenerates it when
note contents change. An existing eligible field with an omitted `value` is
empty; explicit null and missing field identities remain invalid.
Conservative limits are 64 KiB per input field,
1 MiB per raw/serialized item, and bounded 4 MiB subprocess/list captures.
Parsed strings, raw captures, and fixed-capacity serialization buffers have
zeroizing owners; Serde scratch space and compiler/OS copies remain best effort.

The latest configuration lock spans preparation and the item edit. Inputs and
mappings are revalidated after provider reads and serialization. The edit uses
stdin through bounded cancellable subprocess work; authentication/read behavior
outside push is unchanged. Transfer-start evidence lives in the plan, so dropping
an edit future reports an uncertain remote outcome rather than rollback. A final
reread is not a conditional write: the remote race remains. The closed Secure
Note schema does not establish eligibility for other categories. Scoped
[macOS live verification](specs/03-save-files-to-provider.md#macos-verification-evidence-2026-10-19)
records CLI compatibility and outstanding checks, not blanket release approval.

### Interactive local storage

The [spec's unified TUI controls](specs/01-local-storage.md#agreed-tui-management)
replace legacy search/letter/Space actions and Ctrl+P preview with type-to-filter lists,
filter-only Backspace, Ctrl+D deletion, Ctrl+N creation, Enter edit/select,
Ctrl+R reveal/hide (30-second expiry), Ctrl+Y copy, Ctrl+B browsing, Ctrl+S save,
Ctrl+C quit, and F1 help. Esc is the primary dashboard exit, regardless of its
filter. In browsers, Esc clears a nonempty filter first, otherwise goes back
through parent levels to the editor; editor Esc discards the draft. Mapping deletion's optional ciphertext toggle uses
Tab rather than Space. Reference reveal/copy resolves on demand, while manual
and encrypted editors expose only typed drafts. F1 returns to the previous view
on Esc and does not interrupt active work or confirmation dialogs. Reveal can
be cancelled with Ctrl+R while resolving; cancellation rejects stale results.
The spec owns the exact contract and acceptance coverage.

The dashboard and author-capable browser each append a selectable creation row;
Enter on that row and Ctrl+N open a draft. Ctrl+B in an entry editor opens its
provider browser. Local-storage rows contain IDs and fingerprints, including
unlinked records, and select qualified references. The editor calls the reference
field **Local storage ID**. Filtering never removes the browser's creation row.

Encrypted creation edits an ID, value, and SSH fingerprint independently of the
environment name. A sole eligible key is selected automatically; multiple keys
require explicit selection. Text is exact UTF-8, including empty values,
whitespace, and newlines; paste containing NUL is rejected entirely. Like manual
values, drafts start masked; Ctrl+R toggles revelation with a 30-second expiry.
Only draft text is revealed, not an existing encrypted value. Preview shortcuts
perform no fetch or command execution in this editor.

Saving an existing `(ID,key)` requires replacement confirmation and warns that
all references can be affected. Known-reference counts cover only the loaded
configuration. Saving returns to browsing without saving configuration; selecting
a reference then saving the entry links it separately. There is no Ctrl+U action.
Authored references can omit the selector only when exactly one loaded key has
the ID; browser selections always qualify the reference.

Browser record deletion on a selected existing ID/fingerprint row prepares a
snapshot and opens confirmation with Ctrl+D.
Enter/y removes only that encrypted record; Esc/n cancels.
The warning says mappings are kept and references will fail, counts possible
shared references only in the loaded configuration, and notes that other
configurations may also use the record. No agent access, signing, or decryption
is involved, including for unlinked records or keys absent from the agent.
Configuration bytes and the entry draft are unchanged. Record deletion is inert
on the creation row, empty results, and loading. Backspace erases filter text
only; Esc handles parent navigation after clearing a nonempty filter.
Successful removal, including replacement with a subsequent durability error,
removes the exact qualified row from the browser and invalidates fetched results.

Mapping deletion keeps ciphertext by default. Optional removal first selects a
loaded identity without signing or decrypting; after selection, configuration is
saved before ciphertext removal. Qualified references
restrict selection to their fingerprint; unqualified references require exactly
one loaded eligible key with that ID. Missing, ambiguous, or invalid identity or
storage keeps ciphertext while removing the mapping. Other IDs and users are
preserved, and an empty per-key file remains.

Generation tracking rejects stale draft results. Cancellation discards the
zeroizing draft and unfinished work, not completed writes; late storage results
never save an old reference into a new draft. Failure details expose partial
completion and durability warnings rather than claiming rollback.

## Storage / Persistence

Configuration is local JSON in `psst.json`, legacy `.psst.json`, or a path selected
by `PSST_CONFIG`. Manual literals and `.env` exports are plaintext. Configuration
saves use atomic replacement and loaded-content conflict detection.

### Local-storage paths and validation

Per-key ciphertext JSON and lock files live directly in `.psst/` beside the
selected configuration, never beside the caller's working directory. Older
`.psst/local-storage/` files are neither read nor automatically migrated.
Manual relocation must preserve private permissions and avoid filename collisions.

Identity is `(ID, public key)`. IDs are exact, case-sensitive UTF-8, 1–256 bytes,
without `#` or Unicode control characters (`char::is_control`). They are never
trimmed, normalized, or used as paths, even when containing slashes or dots.
Secret text is exact UTF-8, 0–1,048,576 bytes, excluding NUL. Input limits are
checked before requesting signatures.

Only Ed25519 keys are eligible. The canonical SSH public-key blob is
`string("ssh-ed25519") || string(raw 32-byte Ed25519 public key)`, exactly 51 bytes.
Invalid encodings, trailing bytes, invalid points, and weak public keys are
rejected. Its OpenSSH fingerprint is `SHA256:` plus standard unpadded base64 of
SHA256(the complete blob). Its filename is the lowercase 64-character hex of the
same hash plus `.json`; the cooperative lock uses the same stem plus `.lock`.
Neither IDs, comments, nor fingerprint text become filesystem paths.

Storage uses Unix 0700 directories and 0600 files. Existing nonprivate storage
and symlinked paths or ancestors are refused, not silently repaired. Existing
configuration ancestors need not be private. Saves establish missing ancestors
without writing configuration first. Reads are bounded before parsing; each file
has at most 1,024 records and 4,194,304 encoded JSON bytes. Serialization stops at
the byte bound instead of allocating an oversized result. Browsing bounds the
whole scan to 4,096 directory entries and 64 MiB of JSON; exceeding a bound fails
rather than returning an incomplete list.

### Selected v1 JSON format

A per-key document has exactly these required fields:

| Field | Encoding / meaning |
| --- | --- |
| `version` | Integer `1` |
| `public_key` | Canonical padded standard base64 of the 51-byte SSH blob |
| `fingerprint` | Exact OpenSSH SHA256 fingerprint of `public_key` |
| `records` | Array of envelopes with distinct IDs; empty array allowed |

Each envelope has exactly these required fields:

| Field | Encoding / meaning |
| --- | --- |
| `version` | Integer `1` |
| `id` | Exact ID string |
| `public_key` | Same canonical base64 SSH blob as the document |
| `salt` | Canonical padded standard base64 of 32 random bytes |
| `nonce` | Canonical padded standard base64 of 12 random bytes |
| `ciphertext` | Canonical padded standard base64 of ciphertext followed by the 16-byte GCM tag |

Decoded ciphertext plus tag is 16–1,048,592 bytes. Unknown, missing, or duplicate
fields; duplicate IDs; unknown versions; malformed/object-incompatible JSON;
noncanonical base64; invalid sizes; mismatched key/fingerprint/filename; and
oversized files are errors. The writer emits compact JSON, while the parser
accepts bounded JSON whitespace. Property order and whitespace are not
cryptographic inputs.

The complete document and all envelopes are validated before signing or
persistence work. Decryption additionally requires the expected key and ID.
Headers are checked for exact consistency; version/key/ID are cryptographically
bound inside each envelope. The set/order of records is not authenticated:
removal, rearrangement, and replay of previously valid records are not prevented.

### Selected v1 byte encoding

`||` concatenates bytes. `u32` is unsigned four-byte big-endian; `string(b)` is
`u32(byte_length(b)) || b`. Quoted domain strings below are ASCII with an actual
zero byte at `\0`. There are no implicit terminators, delimiters, or JSON inputs.
Salt and nonce have fixed sizes and no length prefixes.

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

These domains are format bytes, not filesystem paths; flattening storage did not
change them. Creation obtains two signatures over the identical challenge,
strictly verifies both, and requires byte-for-byte repeatability. Decryption
obtains and strictly verifies one signature. Signing uses ordinary Ed25519, not
Ed25519ph. Every write uses fresh OS-CSPRNG salt and nonce; randomness failure is
an error. Unsupported signatures, failed authentication, non-UTF-8 plaintext, and
NUL fail without returning plaintext or retrying/falling back.

### Writes, conflicts, and partial outcomes

Per-key saves use cooperative locks, loaded-byte conflict checks, and synced
atomic replacement. They recheck conflicts after signing and preserve unrelated
IDs. Browser creation saves ciphertext independently; linking later saves
configuration. Replacement can already affect existing references even when a
subsequent mapping save fails. Mapping deletion saves configuration before
removing ciphertext; independent browser deletion never saves configuration.
Both removal paths reuse the validated snapshot, private-path checks, cooperative
lock, loaded-byte conflict detection, and synced atomic replacement, preserving
other IDs/users and retaining an empty per-key file. There is no cross-file
transaction or rollback.

Configuration and per-key writes share an owned lock guard that explicitly
unlocks on scope exit, including failure and cancellation. Closing only the
parent's descriptor is insufficient when a concurrent fork temporarily inherits
the same open file description before close-on-exec. Acquisition remains
nonblocking; genuine contention is an error, not an automatic retry.

| Outcome | Observable state / recovery |
| --- | --- |
| Storage failure before replacement | Configuration is not saved; resolve the cause, reload, then retry. |
| Ciphertext replacement followed by sync/reference-validation failure | Existing references may use the new value; configuration remains unsaved. Inspect actual records and the durability warning. |
| Storage saved, later mapping save fails before replacement | Unlinked ciphertext may remain; reopen browsing and link it or deliberately remove it later. |
| Configuration replacement followed by directory-sync failure | The mapping changed with uncertain durability; reload and verify, never assume rollback. |
| Deletion configuration failure before replacement | Mapping and ciphertext remain. |
| Deletion configuration sync failure after replacement | Mapping was removed with uncertain durability; ciphertext is conservatively kept. |
| Mapping removed, ciphertext removal fails | Ciphertext remains unless replacement already completed; partial errors report actual removal and uncertain durability. |
| Browser removal fails before replacement | Ciphertext, configuration bytes, and entry draft remain unchanged; reload before retrying a conflict. |
| Browser removal replacement followed by sync failure | Record removed with uncertain storage durability; configuration bytes and entry draft remain unchanged. |

On lock/conflict errors, reopen configuration and the draft to load current
snapshots rather than overwrite a competing edit. Completed writes survive
cancellation. Records are not automatically cleaned up. Unlinked records can be
removed directly in the browser without a temporary mapping; reference discovery
cannot establish whether configurations outside the loaded file still use them.

## External Integrations

Provider modules support 1Password CLI, Vault KV v2, Terraform outputs, trusted
shell commands, and manual values. External CLIs inherit authentication; psst
does not cache authentication tokens.

Local-storage targets deterministic Ed25519 SSH-agent signing through Unix
`PSST_SSH_AUTH_SOCK` or `SSH_AUTH_SOCK`, including 1Password SSH Key items,
without reading private keys or invoking SSH/encryption executables. Compatible agents are allowed without
vendor checks; a vendor-looking socket alone is not proof of custody. Consented
macOS ARM64 1Password functional checks passed with process/socket ownership and
code-signature identification. Authorization settings remain undocumented, and
Linux live checks and independent crypto/format review remain outstanding.
The macOS results are scoped evidence, not blanket release approval. The
[spec](specs/01-local-storage.md#further-notes) records security and recovery
constraints.

## Testing Seams

Test configuration independently, provider resolution with fake executables,
and CLI/UI behavior with isolated configurations. Normal tests never require
real authentication, remote secrets, personal agents, or private keys. Shared
fake Unix agents use public deterministic seeds and explicit sockets. PTY and
TestBackend cases cover representative workflows, exact input, intentional
revelation/expiry, cancellation, stale results, and terminal restoration.

Focused UI and representative PTY cases exercise immediate filtering with
former shortcut characters, filter-only Backspace, filter-first Esc and parent
navigation, editor discard, dashboard exit with or without a filter, context-appropriate controls,
and consistent help/hints. Coverage checks on-demand reference reveal/copy versus
draft-only manual/encrypted editing, hide/30-second expiry, and no legacy Ctrl+P
action. Deletion cases verify Ctrl+D confirmation/cancellation, Tab's default-keep
ciphertext toggle, exact qualified browser removal without agent access, unchanged
configuration/drafts, inert creation/empty/loading states, and persistence
conflicts and pre-/postreplacement outcomes. These targets implement the spec's
current contracts without changing historical verification or release gates.

The [stable fixture](../tests/fixtures/local-storage-v1.json) decrypts to
`" \n🔑\t"`. It uses public test seed `[7;32]`, ID `fixture`, salt `[1;32]`, and
nonce `[2;12]`. Its filename is
`cff7d2bf46744594be2dc71b73a668396c2dfdf6e3d5514919204a41ba3889c1.json`.
The [offline generator](../tests/fixtures/generate-local-storage-v1.mjs) uses
Node's built-in Ed25519/HKDF/AES-GCM to reproduce the JSON and intermediate
vectors without an agent or key-file access; ordinary Rust tests need neither
Node nor a real agent.

```text
signature = 8336f0db0be5dbe97128aa502b26b5a9fd39bb6c51a81b3b1a32ecf44148f9909
            274678f82a2afd55bc4a2817fcf362d6cd9d3e939e980aef7fbd666a069ee04
key       = 81e0bb0d85aadb08630da834df74405d81b05bde51fca33ff7cea2419798bf0d
```

Focused tests pin AAD/challenge bytes, signatures, derived keys, and fake-agent
decryption. Fault injection uses test-only Dalek hazmat to model two valid but
different signatures; it is not a production signing path or custom primitive.
Persistence injection distinguishes completed replacement from durability
failure. The acceptance inventory and historical command/platform results remain
in the retired tickets' Git history, not in this architectural description.
Neither vectors nor self-tests constitute independent crypto review or verified
1Password compatibility.

## Important Constraints

Linux and macOS are supported. Shell entries execute trusted code; other CLI
integrations use literal arguments. Secrets never appear in diagnostics or
browser metadata. Deliberate short-lived draft/secret revelation is distinct
from accidental disclosure. Command environments, clipboard contents, manual
configuration values, and exports remain plaintext.

No persistent plaintext, signature, or derived-key cache exists. Plaintext and
internal signature/key buffers use zeroizing guards, including failure and
cancellation paths. Encryption reserves tag capacity to avoid freeing an unwiped
plaintext allocation; enabled AES/GHASH/POLYVAL features clean available library
key state. This is best effort: stack copies, HKDF internals, compiler behavior,
OS memory, and compromised processes are not guaranteed protected. Callers own
the input text lifetime. The local-storage API does not place plaintext in argv.

Agent signing authorization grants durable decryption capability: authorized
processes can retain signatures/derived keys after locking. Agent-only private
key custody does not keep derived keys out of psst memory. Prefer per-operation
confirmation; actual prompts depend on agent settings, not a fixed prompt count.
Avoid untrusted forwarding; forwarded-agent detection is not guaranteed.

Public keys, fingerprints, IDs, lengths, and record presence remain visible.
Loss of the SSH key loses decryptability. Manual rotation requires the old key
while creating new records and updating mappings; it cannot revoke Git history
or captured signatures/keys. No metadata hiding, replay/rollback protection,
automatic cleanup/migration/rotation, recovery recipients, multi-recipient
sharing, other key types, private-key reads, plugins, or Windows support is
provided.
