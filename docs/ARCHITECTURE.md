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

## Data Flow

Load configuration → resolve entries through providers → inject successful results into a command, export them, or present them in the console. Resolution preserves configuration order and individual failures; fetches run concurrently except for queued 1Password reads.

## Storage / Persistence

Configuration is local JSON in `psst.json`, legacy `.psst.json`, or a path selected by `PSST_CONFIG`. Manual literals and `.env` exports are plaintext. Configuration saves use atomic replacement and conflict detection.

## External Integrations

Provider modules support 1Password CLI, Vault KV v2, Terraform outputs, trusted shell commands, and manual values. External CLIs inherit authentication; psst does not cache authentication tokens.

## Testing Seams

Test configuration independently, provider resolution with fake executables, and CLI/UI behavior with isolated temporary configurations. Tests must not require real authentication or remote secrets.

## Important Constraints

Linux and macOS are supported. Shell entries execute trusted code; other CLI integrations use literal arguments. Retrieved secrets must not appear in diagnostics or browser metadata. Resolved values reject NUL bytes and use zeroizing storage, but command environments, clipboard contents, manual configuration values, and exports remain plaintext.
