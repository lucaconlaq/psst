# Architecture

## Purpose

psst manages local configuration entries and resolves secrets for interactive use or injection into a command's environment.

## Entry Points

`src/main.rs` starts the interactive console, wraps commands, or handles clipboard copying and `.env` export. `src/cli.rs` parses arguments.

## Technology used

Rust with Tokio for asynchronous work, Ratatui and Crossterm for the terminal UI, and Serde for JSON configuration.

## Main Modules

- `config`: configuration discovery, validation, and persistence.
- `providers`: provider registry and secret resolution.
- `browser`: provider-independent reference browsing interfaces.
- `ui`: interactive editing and presentation.
- `process`: bounded subprocess execution and cancellation cleanup.
- `operations`: clipboard copying and dotenv export.

## Module Boundaries / Seams

Providers own fetching, editor metadata, and optional reference browsing. Each implementation lives in `src/providers/` and implements `Provider`; registration is centralized in `src/providers/mod.rs`. Registry order determines the editor's source-selector order.

The UI consumes shared provider and browser interfaces rather than selecting provider-specific behavior. Browser rows expose reference metadata, not retrieved secrets.

### Extending providers

Implement `Provider` in its module and register it in `PROVIDERS`. Its exact `id`
is persisted as `source`; `reference_label` and `sensitive_reference` own editor
metadata. `browser` optionally returns a shared `Level`. `fetch` receives the
stored value and selected configuration directory and returns `FetchFuture`, a
boxed future yielding a zeroizing string. Shared `providers::fetch` rejects NUL.

Encrypted authoring is separate from reference browsing: `Provider::author`
optionally exposes `Author`. It lists public `Identity` fingerprints with opaque
provider-owned tokens; `prepare` returns a conflict-checked `PreparedSave` snapshot.
The snapshot reports replacement/shared-reference metadata; `save` persists
ciphertext and returns a reference, while `reuse` verifies existing ciphertext
without rewriting it. `prepare_deletion` selects a `PreparedDelete` snapshot
without plaintext; the UI saves configuration before calling `remove`. The
`WriteBoundary` callback is the existing persistence fault-injection seam, not a
user-facing plugin API. These are current implementation interfaces, not an
agreed stable extension API.

Bulk resolution creates one local-storage `Operation` for shared identity listing
and `(ID,key)` fetch deduplication. Single fetches and TUI refetches start fresh
operations. This context is internal, not an added provider trait contract.

## Data Flow

Load configuration → resolve entries through providers → inject successful results into a command, export them, or present them in the console. Resolution preserves configuration order and individual failures; fetches run concurrently except for queued 1Password reads and serialized local-storage agent work. A local-storage failure stops wrapped-command launch and `.env` export; other providers retain warning/omission behavior.

## Storage / Persistence

Configuration is local JSON in `psst.json`, legacy `.psst.json`, or a path selected by `PSST_CONFIG`. Manual literals and `.env` exports are plaintext. Configuration saves use atomic replacement and conflict detection. Experimental local-storage keeps per-key ciphertext beside the selected configuration under `.psst/local-storage/`, using private permissions, bounded validation, symlink refusal, per-key locking and conflict detection. Creation saves storage before configuration; deletion saves configuration before storage. Partial outcomes distinguish completed replacement from uncertain durability; there is no cross-file transaction. See [the v1 contract](local-storage-v1.md).

## External Integrations

Provider modules support 1Password CLI, Vault KV v2, Terraform outputs, trusted shell commands, and manual values. External CLIs inherit authentication; psst does not cache authentication tokens. Experimental local-storage uses Ed25519 signing via Unix `SSH_AUTH_SOCK` without reading private keys or invoking `op`. Only fake-agent compatibility is verified; live 1Password checks remain outstanding.

## Testing Seams

Test configuration independently, provider resolution with fake executables, and CLI/UI behavior with isolated temporary configurations. Tests must not require real authentication or remote secrets.

## Important Constraints

Linux and macOS are supported. Shell entries execute trusted code; other CLI integrations use literal arguments. Retrieved secrets must not appear in diagnostics or browser metadata. Resolved values reject NUL bytes and use zeroizing storage, but command environments, clipboard contents, manual configuration values, and exports remain plaintext.
