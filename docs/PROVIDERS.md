# Adding a provider

Each provider owns its fetching, editor metadata, and optional reference browser:

```text
src/providers/
  mod.rs        Shared Provider interface, registry, lookup, and parallel fetching
  manual.rs     Plaintext literals and masking policy
  shell.rs      Trusted shell commands
  op.rs         1Password, op 2FA, OTP generation, and reference browsing
  vault.rs      Vault KV v2 fetching and reference browsing
  terraform.rs  Terraform outputs
```

`src/browser.rs` defines provider-independent browser rows, navigation targets,
and the `BrowserLevel` interface. `src/ui.rs` renders these interfaces; it does
not choose fetching, labels, or browsers with provider-specific matches.

## Fetching and registration

1. Add `src/providers/example.rs` and implement `Provider` from `mod.rs`.
2. Declare `mod example;` in `mod.rs`.
3. Add an instance to the `PROVIDERS` registry in `mod.rs`. Registry order is the
   editor's source-selector order. Persisted `id()` values must be unique.
4. Add tests using isolated temporary configs and fake executables, never real
   authentication or remote secrets.

A text-reference provider can look like this:

```rust
use super::{FetchFuture, Provider};
use crate::process;
use std::path::Path;

pub(super) struct Example;

impl Provider for Example {
    fn id(&self) -> &'static str {
        "example"
    }

    fn reference_label(&self) -> &'static str {
        "Example reference"
    }

    fn fetch<'a>(&'a self, value: &'a str, dir: &'a Path) -> FetchFuture<'a> {
        Box::pin(async move {
            process::output("example-cli", &["read", value], dir).await
        })
    }
}
```

Register it with `&example::Example`. No UI changes are needed.

Use literal argv for external CLIs, not shell interpolation. `process::output`
provides inherited authentication, config-directory cwd, timeout, output limits,
UTF-8 validation, suppressed stderr, and cancellation cleanup. Shell execution
is intentional only for the shell provider. Never include retrieved values in
errors or debug output.

All registered providers are available by default.

Normal callers use `providers::fetch`, which looks up a registered provider and
rejects NUL bytes in its result. It returns `Zeroizing<String>`.
`providers::all` starts all fetches concurrently, preserving config order and
individual failures. The `op` and `op 2FA` providers share a process-wide async
queue for `op read` calls to avoid concurrent authentication prompts. Other
providers remain concurrent. A read's timeout begins after it acquires the queue;
errors, timeouts, and cancellation release the queue. This does not cache secrets
or authentication tokens, and does not serialize reference-browser commands.

## Optional behavior

- `sensitive_reference()` defaults to false. Return true for stored literal
  secrets: the table masks them, and the editor reveals them only on request.
- `browser()` defaults to none. Return `Some(Level::new(location))` to enable
  Ctrl+B. Implement `BrowserLevel` on a provider-local location type, with a
  title and an asynchronous listing of `Choice` rows. Include the browse hint
  in the reference label.

Browser rows contain display metadata and either `Target::Level` to descend or
`Target::Reference` to select a reference. Never put fetched secret field values
in row labels or details. The UI owns search, selection, the parent stack,
cancellation, and stale-result handling. See `op.rs` and `vault.rs` for examples.

This organization does not change the configuration format, provider IDs,
provider subprocess behavior, or the existing Vault KV v2 limitation.
