# Initial baseline

Source: retrospective code-to-spec reconstruction of [commit dc70795](https://github.com/lucaconlaq/psst/tree/dc70795a7e6b16aa85569a4d46a28e04d2005b78), package version **2.0.0-alpha.3**, immediately before [spec 01 was introduced](https://github.com/lucaconlaq/psst/commit/8cea561118031e93ed22c44c632dd62a82ad994f). There is no source idea: this document records already implemented behavior rather than a newly agreed design.

**Scope:** historical starting requirements, reconstructed from the Rust implementation, tests, README, and CI configuration. The main contract is frozen at the source commit; the explicitly labeled maintenance section records shared fixes implemented afterward but before spec 02, without importing spec 01's feature scope. This is not a claim of fresh acceptance testing or release approval. Later specs supersede this baseline where they explicitly change it, notably [local storage and unified TUI controls](01-local-storage.md) and [managed files](02-managed-files.md).

## Problem Statement

Developers need to associate environment names with secrets from different sources, maintain those associations locally, and use the resolved secrets without repeatedly retrieving and entering them by hand. They also need interactive inspection, clipboard access, and an export for tools that consume dotenv files.

## Solution

Provide a local, ordered configuration of entries and a single `psst` executable. Resolve entries through built-in providers for interactive use, copying, plaintext export, or injection into a wrapped command. Keep external authentication with the existing provider tools and avoid exposing resolved secrets in ordinary diagnostics.

## User Stories

1. As a developer, I want project-local entries discovered automatically, so that commands use the appropriate configuration without repeated path arguments.
2. As a developer, I want references, trusted shell commands, and manual literals resolved through one interface, so that different secret sources work together.
3. As a developer, I want to run a command with resolved secrets, so that my tools receive the required environment without shell interpolation or manual exports.
4. As a developer, I want to copy one secret or export a dotenv file, so that I can use tools that do not run through the wrapper.
5. As an interactive user, I want to add, edit, browse, inspect, and remove entries, so that I can maintain configuration without hand-editing JSON.
6. As a user downloading psst, I want tested platform-specific release archives and checksums, so that I can select and verify the appropriate executable.

## Implementation Decisions

The contracts below describe the selected historical implementation, including its limitations; they are not proposals to change current code.

### Configuration discovery and representation

- A nonempty `PSST_CONFIG` selects the configuration path, even if the file does not exist. Relative overrides are relative to the caller's working directory; absolute overrides remain absolute. An empty override behaves as unset.
- Otherwise search the caller's directory and its ancestors, checking `psst.json` before `.psst.json` at each level. The nearest match wins. Stop after checking the directory supplied by `HOME` if encountered, otherwise at the filesystem root; there is no account-database home lookup. With no match, select `psst.json` in the caller's directory.
- The baseline JSON API is an unversioned root object mapping environment names directly to entries, with no wrapper or managed-file section. Preserve root insertion order. The canonical entry representation is an object with this contract:

  | Field | Type | Contract |
  | --- | --- | --- |
  | `source` | Required string | Exact, case-sensitive provider identifier. Unknown identifiers remain loadable/editable but cannot resolve or browse. |
  | `value` | Required string | Provider reference, trusted shell command, or manual literal, interpreted according to `source`. |

- Historical deserialization also accepts a positional entry array containing exactly two strings in `source`, `value` order. Saving normalizes it to the canonical object representation. This incidental compatibility does not permit a root array.
- Environment names are case-sensitive and match ASCII `[A-Za-z_][A-Za-z0-9_]*`. Reject duplicate environment names, including equivalent decoded JSON spellings; duplicate recognized `source`/`value` fields; malformed JSON/schema; missing or incorrectly typed required fields; and NUL in either retained string. Empty strings are configuration-valid; provider resolution may reject them. Otherwise retain stored text exactly, without trimming or normalization.
- Ignore unknown entry fields, including repeated unknown fields and arbitrary valid JSON values; they are not retained on save or subject to retained-string NUL checks. This is not a lossless arbitrary-JSON editor.
- Missing and exactly zero-byte configuration files are empty configurations. Loading never creates a file. Nonempty malformed files, including whitespace-only input, fail rather than silently becoming empty. Bound both loaded and saved configuration to **4,194,304 bytes**, including JSON whitespace.
- Provider operations use the selected configuration's directory, independently of the caller's directory. Loading configuration does not resolve its entries.

### Configuration persistence

- Validate entries before saving, create missing parent directories, and serialize ordered entries as pretty JSON.
- Use a nonblocking cooperative exclusive lock at the configuration path with its extension replaced by `psst-lock`. Contention is an error, not a retry loop.
- Compare current file bytes, including missing-versus-existing state, with the loaded snapshot. External changes require reopening rather than silently overwriting.
- Write through a temporary file in the destination directory, sync its contents, atomically replace the destination, and sync the parent directory on Unix. Replacement files have Unix mode **0600**.
- Atomic replacement is not rollback: a directory-sync error can occur after the destination changed. Cooperative locks do not prevent races from writers that ignore them. This baseline does not impose the later encrypted-storage private-path or symlink protections on configuration.

### Provider resolution

All six providers are available by default with these exact, case-sensitive JSON `source` identifiers: **`op`, `op 2FA`, `vault`, `shell`, `manual`, `terraform`**.

| Source | Stored value and resolution |
| --- | --- |
| `op` | Require an `op://` prefix; run `op read REFERENCE` with literal arguments. Reject an empty result. |
| `op 2FA` | Read the same way, then generate an OTP locally from the returned `otpauth` URI. |
| `vault` | Interpret `path#field`; run `vault kv get -format=json PATH` and select the literal field key in the KV v2 response's `data.data` object. Missing fields fail. |
| `shell` | Execute the stored command as trusted code through `/bin/sh -c` in the configuration directory. Successful empty output is valid. |
| `manual` | Return the literal unchanged, including whitespace and newlines; reject the exactly empty string. |
| `terraform` | Interpret `directory#output`; run `terraform output -json` in the derived directory and select the named output's `value`. Missing output/value fails. |

- Vault and Terraform require nonempty path/directory and field/output components. The historical parser uses the first two `#`-separated components and ignores subsequent components; it has no escaping syntax.
- Terraform strips leading `/` characters from its directory before joining it to the configuration directory. It does not confine `..` or symlinks: configuration-relative resolution is not a filesystem sandbox.
- Selected Vault/Terraform strings are returned without JSON quoting and retain their string contents. Other selected JSON values, including objects and arrays, are serialized as JSON. Empty JSON strings are successful results.
- OTP generation uses the first `secret` query parameter of an `otpauth` URI, rejects a missing or initially empty parameter, removes trailing Base32 padding, uppercases the seed, and rejects Base32 decoding errors. Historically, padding-only input normalizes to empty bytes without another emptiness check and generates a code with an empty HMAC key. Generate **six-digit HMAC-SHA1 codes with 30-second periods** from current Unix time. URI algorithm/digits/period options are ignored; HOTP counter semantics are not implemented.
- Shared dispatch rejects unknown sources and any resolved NUL, including NUL introduced by JSON decoding.
- Resolve a batch concurrently while preserving configuration order and individual failures. Provider subprocesses inherit the original environment, not secrets resolved by other entries.
- Both 1Password sources share one process-wide queue around `op read` to avoid authentication races. Other providers remain concurrent. Authentication belongs to the external CLI; psst does not cache authentication tokens.

### Subprocess and timeout boundary

- Except for the explicitly trusted shell provider, pass arguments literally rather than interpolating them through a shell. Resolve external executables using inherited `PATH` and inherit their authentication environment.
- Provider subprocess stdin and stderr are null; capture stdout. Require successful exit, UTF-8, and no NUL, then trim leading and trailing Unicode whitespace. This trimming applies to subprocess output, not manual literals or the contents of a subsequently decoded JSON string.
- Bound each captured subprocess response to **4,194,304 bytes before UTF-8 decoding**, including complete JSON/browser responses. Reject overflow rather than use partial output.
- `PSST_TIMEOUT` is a positive integer number of seconds; missing, invalid, or zero values use **30 seconds**. Each subprocess's output-read/wait phase has its own deadline. The 1Password queue wait is outside that deadline; this is not a whole-batch deadline.
- On Unix, run provider subprocesses in owned process groups and kill remaining group members on completion, error, timeout, or cancellation. This cleans up ordinary descendants, not processes deliberately escaping the group. No automatic retry is provided.
- Suppress external stderr and avoid echoing captured output or stored values in resolution errors. Configuration schema diagnostics report location/context without reproducing manual literals.

### Command-line modes and wrapped commands

| Invocation | Public behavior |
| --- | --- |
| `psst` | Open the interactive manager. |
| `psst COMMAND [ARGS...]` | Resolve entries and execute the command with the successful results injected. |
| `psst -- COMMAND [ARGS...]` | End wrapper-option parsing explicitly, including for executables beginning with `-`. |
| `psst --copy NAME` or `psst --copy=NAME` | Resolve and copy exactly the named entry. |
| `psst --write` | Export resolved entries to the caller's `.env`. |
| `psst -h` or `psst --help` | Print usage to stdout without loading configuration when help is the selected mode. |
| `psst -V` or `psst --version` | Print `psst VERSION` followed by a newline to stdout without loading configuration. |

- No arguments after an empty `--` delimiter still selects the interactive manager. There are no management subcommands or additional wrapper options.
- Parse wrapper options only until the first unrecognized argument or `--`; preserve the remaining native executable/arguments verbatim. Downstream flags are not reinterpreted as psst options. Copy requires a nonempty name; its separate-argument form rejects a name beginning with `-`.
- A version flag reached successfully during wrapper parsing returns immediately; it does not override an earlier parsing error or a command boundary. Otherwise mode precedence is copy, write, help, then command/interactive mode. Repeated copy options select the last parsed name. A selected copy/write/help mode ignores any collected command. A non-UTF-8 copy name fails at dispatch, whereas wrapped-command arguments retain native bytes.
- For wrapping, warn once per failed entry and continue with successful results. Overlay successful values, including empty strings, on the inherited environment. Failed entries do not erase inherited variables. Even zero successful entries do not prevent launch.
- Launch in the caller's original working directory with inherited stdin/stdout/stderr. An injected `PATH` also determines lookup of an unqualified wrapped executable; provider executables and clipboard helpers still use psst's original environment. On Unix, replace psst with the command using `exec`, retaining native argument bytes, process identity, exit status, and signal/job-control behavior. Launch failure is an error.
- During noninteractive resolution/copy, Ctrl+C and SIGTERM cancel pending work and return **130** and **143**, respectively. Other top-level failures return **1**; successful copy/export/help/version return **0**.
- Put operational status messages and diagnostics on stderr, leaving wrapped-command stdout clean; help/version are the informational stdout exceptions described above. Report successful-result counts, including successful empty values. Color terminal stderr by message severity; the presence of `NO_COLOR` (even empty or `0`), an exact `TERM=dumb`, or nonterminal stderr disables colors.

### Clipboard and dotenv export

- Copy resolves only the named entry and propagates missing-entry, resolution, and clipboard errors. Reject empty values. Send literal UTF-8 bytes through helper stdin without adding a newline or interpreting shell syntax.
- On macOS use `pbcopy`. On Linux when `WAYLAND_DISPLAY` is present, even empty, try `wl-copy`, then `xclip -selection clipboard`, then `xsel --clipboard --input`; otherwise begin with `xclip`. Fall through only when a helper executable is missing, not after a started helper fails.
- Apply `PSST_TIMEOUT` to clipboard writing/waiting. Timeout or cancellation cleans up the helper group; successful clipboard owners may outlive psst. Clipboard contents are not automatically cleared.
- `--write` atomically overwrites **`.env` in the caller's directory**, not the configuration directory, using the same synced replacement mechanism and Unix **0600** mode.
- Export successful nonempty values in configuration order as `NAME=VALUE` lines with a final newline per entry. Warn about and omit failures; omit successful empty strings. No eligible values produces an empty export, not an error.
- Emit nonempty values containing only ASCII alphanumerics or `_./:@%+,-` unquoted. Otherwise double-quote and escape backslashes, double quotes, dollars, and line feeds. Preserve literal data for dotenv consumers; the result is not a shell script and must not be sourced.

### Interactive management

This section specifies capabilities and safety behavior, not keybindings or screen layout. Later specs own their explicit interaction changes.

- Require terminal stdin and stdout, with a useful error directing nonterminal callers to noninteractive modes. Provide entry navigation, case-insensitive environment-name filtering, in-app help, context-appropriate action guidance, and exit/cancellation. Help explains the plaintext and clipboard caveats. Remain usable with an empty configuration and show a resize request when the terminal is too small.
- Present the selected configuration path and ordered entries with their environment names, sources, references, and masked secret state. Opening the manager or an editor must not automatically fetch secrets or execute shell commands.
- Create, edit, rename, and delete entries. New drafts default to the manual provider and a generated name. Edit environment name, source, and stored value with Unicode-aware cursor editing and bracketed paste. Historical paste handling strips NUL; this is an observation, not a requirement to preserve it where later specs require rejection.
- Keep drafts unsaved until explicit save; cancellation discards the draft. Validate environment names and prevent renaming over another entry. Do not require provider resolution or a nonempty stored value to save configuration. Preserve an existing entry's position when updating it; newly added or renamed entries append.
- Require confirmation before deleting an entry, with cancellation leaving configuration unchanged. Deletion changes configuration only, never provider-side content.
- Retain drafts after save errors and report validation, persistence, provider, clipboard, and browser failures without making the manager unusable. Show asynchronous loading states rather than blocking input.
- Ignore stale asynchronous results after the relevant draft/view changes and cancel abandoned work on shutdown. Returning to the main view clears revealed values. Restore terminal state after normal exit, errors, and explicit interruption, with a panic cleanup path.

### Interactive secret visibility

- Mask resolved secrets by default. Also mask manual stored values and unknown-provider references in the entry list; ordinary provider references and shell commands remain visible.
- Offer explicit on-demand secret resolution, reveal/hide, and clipboard copying. Revealed secrets expire after **30 seconds**. Copy may reuse a currently revealed value or resolve it without displaying it; allow one copy operation at a time and warn that the clipboard is not automatically cleared.
- Mask manual literals in the editor, with explicit reveal/hide and **30-second** expiry. Other providers' stored values, including unknown providers, are immediately visible there; entry-list masking does not imply editor masking.
- Offer an explicitly requested resolved editor preview with **30-second** expiry. A preview belongs to its draft snapshot: editing text, changing source, selecting a reference, or requesting a replacement preview invalidates the old preview and outstanding results. Previewing a shell entry executes its command only on explicit request; typing or opening the editor does not. This describes the baseline preview capability, not a requirement to restore a later-removed preview action.
- Replace control characters with spaces in rendered table contents, stored-value/preview text, browser row labels/details, and statuses. Historical exceptions exist for browser titles and editor environment-name drafts; sanitization is not a universal terminal-safety guarantee.

### Provider reference browsing

- Offer hierarchical browsing from entry editors for `op`, `op 2FA`, and `vault`; other sources use text input. Support local case-insensitive label filtering, parent navigation, cancellation back to the unchanged draft, and visible loading/empty/error states.
- Browser rows expose labels, detail metadata, and navigation/reference targets, never rendered field values. This is a presentation boundary, **not a promise that external responses contain only metadata**: item/secret inspection commands may return complete secret-bearing JSON.
- Selecting a reference updates only the draft and clears its preview. Explicit entry save is still required to persist it; selecting a reference does not itself resolve the secret.
- 1Password browsing visits vaults, items, then fields using `op vault list --format json`, `op item list --vault ID --format json`, and `op item get ID --vault ID --format json`. Use names/titles and field labels/types; fall back to field IDs for missing labels and omit fields without a nonempty reference. Browser requests do not use the `op read` queue.
- Vault browsing starts with `vault read -format=json sys/internal/ui/mounts` and includes mounts of type `kv`, without checking their KV version. At each path concurrently attempt `vault kv list -format=json PATH` and `vault kv get -format=json PATH`. List keys become navigable rows before field rows; fields select `/PATH#FIELD` references. Tolerate either request failing; both failing produces an error. Successfully parsed path responses with unusable shapes may yield no rows rather than a validation error. Fetching and field extraction still require KV v2 response structure.

### Architecture, confidentiality, and delivery

- Use Rust with Tokio, Serde JSON, Ratatui, and Crossterm. Separate configuration, provider resolution, provider-owned browsing, subprocess lifecycle, user operations, and interactive presentation. A shared provider registry owns source lookup, editor metadata, and optional browsing; it is not a stable external plugin API.
- At this baseline, the package uses Rust edition **2024** and declares minimum Rust **1.88**. Historical CI uses moving stable Rust, not a pinned minimum-version verification job.
- Use best-effort zeroizing ownership for captured output, resolved strings, and sensitive drafts. Do not promise comprehensive memory erasure: parsed JSON and intermediate copies are not uniformly protected. Manual configuration, exported files, injected environments, clipboard contents, and intentional reveals remain plaintext.
- Support Linux and macOS on x86_64 and ARM64. On branch pushes and pull requests, check formatting, run Clippy with warnings denied, and test/build the four release targets. Linux release binaries use musl.
- For `v`-prefixed version tags, require an exact package-version match. Publish a GitHub Release only after checks and all target tests/builds succeed, with `psst-TARGET.tar.gz` archives containing the executable and a `SHA256SUMS` file. Targets are `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-musl`, and `aarch64-unknown-linux-musl`. Tags with a prerelease suffix are prereleases, not the latest stable release. No npm or crates.io publication is provided. Distribution is MIT-licensed; the repository license owns its legal terms.

### Shared maintenance inherited before spec 02

The following are explicit amendments to the frozen baseline, verified in the history through [the introduction of spec 02](https://github.com/lucaconlaq/psst/commit/20f5162). They apply to existing functionality, not just the new local-storage provider. Spec 01 continues to own encrypted storage and its TUI changes.

- [Alpha.4 shared persistence changes](https://github.com/lucaconlaq/psst/commit/2561a3e) replace the old configuration-lock naming rule: append `.lock` to the complete filename, so `psst.json` uses `psst.json.lock`, legacy `.psst.json` uses `.psst.json.lock`, and arbitrary override filenames follow the same rule.
- The same change distinguishes persistence failures before versus after destination replacement. Before replacement, configuration's loaded entries and snapshot remain unchanged; after replacement, update them to the actual new bytes even if directory sync fails. Report changed content and uncertain durability rather than rollback. The shared export writer also reports whether replacement occurred.
- The same change starts shared-runner provider and browser subprocesses in a fresh Unix session, without a controlling terminal, so they cannot bypass redirected streams through `/dev/tty`. This strengthens the baseline subprocess boundary; it does not detach the wrapped command or clipboard helpers from their existing behavior and is not a sandbox against deliberate terminal-device access.
- [Lock-lifetime hardening](https://github.com/lucaconlaq/psst/commit/def42d6) explicitly releases cooperative locks at operation end, including failure/cancellation paths, even when another process temporarily inherits a descriptor. Genuine contention remains a nonblocking error. Ordinary TUI save, reveal, copy, and browse failures also preserve nested error causes for the details presentation owned by spec 01.
- [Clipboard diagnostic hardening](https://github.com/lucaconlaq/psst/commit/1154f18) gives a helper pipe-write failure useful clipboard-specific context without leaking the secret or writing diagnostics to stdout. It remains an error, not a reason to try another helper after one started.
- [Toolchain pinning](https://github.com/lucaconlaq/psst/commit/2c0a372) replaces historical moving-stable verification with pinned Rust **1.88.0** locally and in CI. Current toolchain ownership remains in the [Rust rules](../rules/RUST.md#rust-9-style-editions-and-toolchain-compatibility), not a duplicate version-upgrade policy here.

## Testing Decisions

This section inventories existing verification seams and the behavior they cover. It does not claim the historical suite was rerun while writing this spec, or that every documented branch has exhaustive test coverage.

- **Configuration:** focused tests cover discovery precedence, overrides, visible/legacy filenames, parent/home/root traversal, lazy creation, validation, ordered persistence, and external-change conflicts. CLI tests reject malformed/duplicate configurations and check that diagnostics do not expose manual literals.
- **Providers and subprocesses:** use isolated configuration directories and fake executables. Cover exact source registration, manual preservation/empty rejection, trusted shell resolution, literal external arguments, Vault object/Terraform scalar selection, OTP vectors, unknown sources, decoded NUL, unsuccessful exits, invalid UTF-8, oversized output, and whitespace trimming.
- **Wrapper:** binary-level tests cover native argv/stdin, inherited versus overridden environment, provider versus caller working directories, partial failures, empty results, child exit/signal behavior, parallel starts, the shared 1Password queue, timeout queue release, cancellation, and descendant cleanup.
- **Clipboard/export:** fake clipboard helpers verify literal bytes, helper lifecycle, timeout/cancellation cleanup, and successful owner retention. Filesystem and dotenv-parser tests cover private export permissions, order, escaping, omission, and round-tripping without interpolation.
- **Interactive boundary:** real PTY tests exercise adding, paste, saving, masking, reveal/hide, confirmed deletion, shell-editor nonexecution, representative Vault/1Password browsing, explicit reference saving, and terminal restoration, including SIGTERM. Focused state/TestBackend cases cover navigation, draft cancellation, stale results, masking, editing, preview/reveal expiry, empty states, and help.
- **Evidence limits:** test names alone do not establish complete workflows; for example, the historical 1Password browser test does not actually exercise its named cancellation branch. Inspected coverage does not establish every browser error/schema, every timeout/input boundary, comprehensive memory cleanup, or real-service authentication compatibility. Fake tools and PTYs are not live integration evidence.
- **Historical release gate:** formatting, Clippy, locked tests, and release builds across the four platform targets precede release publication. Moving-stable results alone do not prove the declared Rust minimum; the maintenance amendment records the later pin. Installation documentation was still pending, not an implemented installer contract.
- **Shared maintenance regressions:** later tests cover full-filename lock paths, contention and unlock with duplicated descriptors, cancellation/error cleanup, injected failures before/after replacement, provider `/dev/tty` isolation, nested failure details, and clipboard helpers closing stdin without disclosure. These support the maintenance amendments, not a claim of fresh execution.

## Out of Scope

- Encrypted local storage, SSH-agent signing/decryption, local storage IDs, and encrypted-value authoring or removal; these belong to spec 01.
- Managed files, filesystem materialization, file-destination configuration, and managed-file workflows; these belong to spec 02.
- Windows support, external provider plugins, a stable provider extension API, or a standalone provider-management CLI.
- Provider-side creation/update/deletion, authentication-token management, secret dependency ordering, automatic retries, or a persistent resolved-secret cache.
- Sandboxing trusted shell commands, guaranteeing complete memory erasure, encrypting manual configuration or exports, or automatically clearing the clipboard.
- A completed installation workflow, npm/crates.io publishing, or a claim that the Rust baseline reproduces every incidental behavior of all earlier JavaScript/TypeScript versions.

## Further Notes

- The primary historical boundary is the parent of the commit that first created spec 01, not today's code with later features removed. The initial Rust rewrite and all runtime fixes before that boundary are included; later shared maintenance is separately sourced above.
- Completeness audit: all **51 tracked files** at that boundary were accounted for. All **15 runtime files and seven test/fixture files** were read in full; manifests, release configuration, README, and domain/architecture documentation were checked for additional product requirements. The remaining assets, dependency inventory, and development/agent workflows introduce no additional product capabilities. Intervening commits up to spec 02 were also checked for shared maintenance outside spec 01's feature scope.
- Later specs replace the legacy TUI controls and introduce exact-text/NUL rules for their specified editing flows. Spec 01 adds fail-stop resolution and queue-inclusive agent deadlines for local storage; it does not change baseline providers' best-effort policy, consumer-specific empty-value handling, or queued 1Password-read deadlines. Apply each later contract only within its stated scope.
- Positional JSON entries, extra-`#` parsing, padding-only OTP seeds, Terraform path handling, incomplete rendering sanitization and secret-buffer cleanup, and browsing's secret-bearing external responses are historical compatibility observations or limitations, not newly approved security guarantees or recommendations. Recording them neither mandates preserving defects nor authorizes code changes.
- Project-agent workflows, editor settings, and planning documents are development tooling, not additional product capabilities. This reconstruction creates no implementation tickets and authorizes no code changes.
