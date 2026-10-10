# psst Rust rewrite notes

## Scope and evidence

This is a behavior/architecture inventory of the current repository (package version **0.7.3**), not a proposal to port React/JavaScript literally. All repository files outside `.git/` were inventoried and read; the demo image was inspected through the image reader (not played frame-by-frame). Generated dependencies and build output are absent. Notes are based on source and existing tests; tests and real provider integrations were **not run**. No external authentication or secret retrieval was attempted.

Preserve the useful public contracts below. Sections explicitly labeled bugs/decisions describe existing behavior, not recommendations to reproduce unsafe behavior. The existing uncommitted change to `mise.toml` was left alone.

## 1. Complete file inventory and file-by-file notes

Paths below are the full pre-rewrite working-tree inventory, excluding Git internals. `REWRITE.md` is the new documentation file added by this study.

### Packaging, documentation, and development

| File | Responsibility and rewrite relevance |
| --- | --- |
| `README.md` | Public introduction, npm/npx/mise installation, no-argument console and command-wrapper examples, config discovery description. Only claims macOS testing. Documents op, Vault, shell; does not document manual, 2FA, export, clipboard flags, or experimental Terraform. |
| `.github/demo.gif` | Terminal demo asset referenced by README. Image reader exposes a terminal frame; not a normative behavior specification. |
| `LICENSE` | MIT license, copyright 2025 Luca Querella. Keep attribution; resolve disagreement with package metadata's ISC label. |
| `package.json` | Package `@lucaconlaq/psst`, GitHub `lucaconlaq/psst`, version 0.7.3, command `psst`. Publishes `dist` and launcher. Runtime dependencies: Commander (CLI), Ink/React (TUI), speakeasy (TOTP), plus apparently unused `@inkjs/ui`. Build/typecheck via TypeScript; tests via Vitest; format/lint via Biome. Release scripts bump version, build, publish public npm package without Git tag. Rust replaces this tooling, not the user command. |
| `package-lock.json` | Complete npm lockfile v3; pins runtime Commander 11.1.0, Ink 4.4.1, React 18.3.1, speakeasy 2.0.0/base32.js 0.0.1 and all transitive/development/platform dependencies. No additional application features/configuration hidden here. Useful for checking old parser/TOTP/library defaults during parity tests. |
| `.npmrc` | npm publication authentication via `NPM_TOKEN` environment substitution; no embedded token. Build/release concern only. |
| `psst` | Executable Node shebang and import of `./dist/main.js`; published command entry point. Rust should supply executable named `psst`. |
| `psst-dev` | Runs `npx tsx src/main.ts "$@"`; forwards arguments. First line is `# /bin/bash`, not a valid shebang; tsx is not a declared dev dependency. Development artifact, not user API. |
| `.gitignore` | Ignores `dist/`, `node_modules/`, `psst.json`, `mise.local.toml`, `terraform/`. Does **not** ignore `.psst.json` or generated `.env`, despite their potential secrets. |
| `mise.toml` | Local tools: psst 0.7.3, pi latest, rust latest; aliases psst/pi to npm packages. This file already has user modifications. Not runtime config. |
| `tsconfig.json` | ES2020/NodeNext, strict checking, React JSX, `src` to `dist`; tests excluded from compilation. Explains why test props can lag production interfaces. No runtime policy. |
| `biome.json` | Formatting/lint conventions: tabs, 120 columns, double quotes, import organization/type-only imports/extensions. Replace with Rust tooling if needed. |
| `.claude/commands/commit.md` | Contributor command instructions for conventional commits, staged-only changes, user approval. No application behavior. |

### Core application

| File | Responsibility and rewrite relevance |
| --- | --- |
| `src/main.ts` | Creates CLI and parses process arguments. |
| `src/command.ts` | Composition root: flags, version, provider registry/experimental gate, discovers/loads config, dispatches copy/export/help/TUI/run modes. See CLI contract. |
| `src/types.ts` | Domain model: map of environment names to `{source,value}`; provider interface with fetch, safe display, reference label, editor and key hints; success-or-warning result. Static source union omits runtime `op 2FA` and `terraform`; do not copy that inconsistency. |
| `src/config.ts` | Config search and load; no schema validation, merging, expansion, or explicit error diagnostics. Contains root-loop and JSON exception bugs detailed below. |
| `src/injection.ts` | Sequential provider dispatch into a supplied environment map, best-effort failures, injected-count reporting. Catches thrown fetch errors. |
| `src/runCommand.ts` | Direct executable/argv spawn, inherited stdio, supplied environment, current caller directory, exits with child code. Spawn errors exit 1; signal exit maps null code to 0. |
| `src/clipboard.ts` | Fetch one named config entry, validate nonempty result, copy with shell `echo ... | pbcopy`, report and exit. macOS-only, unsafe interpolation. |
| `src/writeEnvFile.ts` | Fetch all entries concurrently, omit unknown/warning/empty results, write unescaped `KEY=value` lines to caller's `.env`, exit 0. Thrown provider errors abort. |
| `src/console.ts` | Yellow ANSI warning on stderr prefixed `⚠️  `; green ANSI info on stdout. Colors emitted unconditionally. |
| `src/utils/tryCatch.ts` | Async result wrapper returning data/error instead of throwing. Used for file reads; cannot catch an expression throwing before invocation (`JSON.parse`). No Rust-specific structure to preserve. |

### Terminal application

| File | Responsibility and rewrite relevance |
| --- | --- |
| `src/Console/Console.tsx` | Top-level main/help/edit state, config-path header, immediate persistence, add defaults, rename/delete orchestration. Creates config parent directories as necessary. |
| `src/Console/MainView.tsx` | Selection, reveal/hide, clipboard, delete confirmation, warnings, per-secret loading and revealed-value cache. Main keybindings. |
| `src/Console/Secrets.tsx` | Four-column table with safe reference vs fetched secret, eight-bullet mask, 30-character truncation and loading/deletion/selection styling. |
| `src/Console/Editor.tsx` | Draft name/source/value, auto-fetch preview, save/cancel, nested provider editor. Currently passes original rather than draft secret into nested editor. |
| `src/Console/EditorForm.tsx` | Four fields, navigation, source Tab cycling, name editing, Enter/Left save, Escape cancel, Right opens provider editor from value/secret fields. |
| `src/Console/ValueEditor.tsx` | Provider-editor delegation with name, secret, config directory, completion callback; rounded frame. |
| `src/Console/Help.tsx` | Usage/examples and console keyboard help; `h` returns or exits standalone help. Interactive help, not ordinary print-and-exit CLI help. Omits copy/export/source-editor details. |

### Providers and their user interfaces

| File | Responsibility and rewrite relevance |
| --- | --- |
| `src/sources/manual/manual.ts` | Literal plaintext provider, rejects empty string, always masks stored value in main table. |
| `src/sources/manual/ManualEditor.tsx` | Append/delete text editor; displays plaintext; Left completes. Clipboard paste is merely normal input, no explicit paste API. |
| `src/sources/shell/shell.ts` | Runs configured shell command in config directory with parent's environment; trims stdout, suppresses stderr, returns warning on failure. |
| `src/sources/shell/ShellEditor.tsx` | Append/delete command editor, Down runs preview showing output/warning, Left completes with command. |
| `src/sources/op/op.ts` | Checks `op --version`, validates `op://`, runs `op read`, trims and rejects empty result. Raw reference is safe display. |
| `src/sources/op/OpEditor.tsx` | 1Password vault → item → field browser; CLI JSON APIs, item/field search, five-row window, selects field-provided reference. Shared by op and op 2FA. |
| `src/sources/op-2fa/op-2fa.ts` | Exact source name `op 2FA`; same op retrieval then otpauth URL parsing and local base32 TOTP generation. Not `op --otp`. |
| `src/sources/vault/vault.ts` | Provider adapter around Vault path fetch, converts errors to warnings; raw path display, mount/path browser. Declared version/read metadata not actually used for fetching. |
| `src/sources/vault/vaultFetchData.ts` | Vault CLI JSON integration: discover KV mounts, list folders/read field names, resolve `path#key`. Assumes KV v2 response nesting. |
| `src/sources/vault/VaultEditor.tsx` | Searchable KV mount picker, loading/auth warning, delegates selected path to nested browser. Left at mount root completes with empty string. |
| `src/sources/vault/VaultPathEditor.tsx` | Searchable folders + keys browser, path join/parent logic, five-row viewport, selects `/<path>#<key>`. Contains empty-selection/input-handler issues. |
| `src/sources/terraform/terraform.ts` | Experimental-only Terraform outputs provider, resolves directory relative to config directory and fetches `output -json`; selects output `.value`. |
| `src/sources/terraform/TerraformEditor.tsx` | Placeholder text “Not implemented yet. Coming soon.”; not a functional browser. Right completes empty value only when original value is empty; advertised Left not implemented. |

### Existing tests (each read)

| File | Established coverage / limitations |
| --- | --- |
| `test/config.test.ts` | Override, both filenames, current/parent/home search, nearest config, fallback. One test title says hidden filename preferred, but assertion correctly expects **psst.json**. Does not test load/errors/outside-home termination. Mutates cwd/HOME and does not fully restore them. |
| `test/injection.test.ts` | Single op/shell/manual fetch, invalid op failure, multiple entries, empty config, partial failures. Substitutes echo for op; does not exercise actual 1Password. |
| `test/sources/op/op.test.ts` | Name/reference label, safe reference, fake echo read, missing executable warning, invalid prefix, editor presence. |
| `test/sources/shell/shell.test.ts` | Metadata, raw command display, echo execution, missing-command warning, editor presence. Does not assert cwd/env/empty-output semantics. |
| `test/sources/manual/manual.test.ts` | Metadata, fixed-length masking, empty warning, successful literal, editor presence. |
| `test/Console/MainView.test.tsx` | Delete y/n, arrow selection/edit callbacks, reveal/loading/hide, add/help callbacks. Missing required configPath prop; add test starts null and does not prove callback happened. No clipboard/empty-table/error/cache coverage. |
| `test/Console/Secrets.test.tsx` | Table contains both names only; no comprehensive rendering assertions. |
| `test/Console/Help.test.tsx` | Help contains “Usage”; title claims branding but does not assert it. |

No existing tests for CLI parsing, child lifecycle, config loading/persistence, editor forms or provider browsers, copy/export, Vault, TOTP, or Terraform. No CI workflow in this tree.

## 2. Architecture to preserve

psst is a **local secret-reference manager and process-environment wrapper**, not a server or encrypted vault. External CLIs own authentication and remote access; psst stores references (or plaintext manual values), resolves them on demand, and exposes them through a child environment, clipboard, env file, or terminal preview.

```text
argv + inherited environment + caller cwd
                 |
       CLI composition/dispatch
                 |
    discover config -> load JSON -> ordered secret entries
                 |
          provider registry
          /      |       \
 interactive  resolve    resolve-all
 CRUD/browser   one      /         \
 + persistence clipboard child env  .env export
```

Core boundaries:

- Configuration repository: chosen file, its directory, ordered entries, load/save.
- Provider registry: exact source identifiers; resolve `(entry, environment name, config directory)` to success(value) or warning(message).
- Provider UI metadata: safe display, reference label, editor key hints, editor workflow.
- Operations: inject all, resolve/copy one, export all; distinct failure/empty policies currently exist.
- Process execution: direct argv child vs intentional shell evaluation for shell-provider commands.
- TUI state: main/help/edit/provider-editor, preview requests, reveal/loading/cache/confirmation.
- Platform adapters: terminal, clipboard, filesystem, clock/TOTP, subprocess executor.

Default provider order is **op, op 2FA, vault, shell, manual**. Append **terraform** when `PSST_EXPERIMENTAL` is any nonempty string (even `0` or `false`). Registry order is source-selector order. No plugin discovery, provider-specific settings, executable override flags, or credentials stored by psst. Executable fields are internal/test seams only.

## 3. Configuration contract

### File format

A single JSON object maps environment names to secret entries; no version field or wrapper:

```json
{
  "API_KEY": { "source": "op", "value": "op://vault/item/field" },
  "OTP": { "source": "op 2FA", "value": "op://vault/item/one-time password" },
  "DB_PASSWORD": { "source": "vault", "value": "/secret/project#password" },
  "TOKEN": { "source": "shell", "value": "./get-token.sh" },
  "LOCAL_SECRET": { "source": "manual", "value": "plaintext" },
  "TF_SECRET": { "source": "terraform", "value": "terraform#output_name" }
}
```

`value` is the reference/command/literal, not normally the resolved secret. Source strings are case-sensitive, including the space/capitalization in `op 2FA`. Unknown sources remain visible in the table but cannot resolve. Terraform configs are unknown-source configs unless experimental mode is enabled.

No name validation, duplicate-name protection, required-field/type validation, interpolation, encryption, inheritance/merge, accounts/profiles, credential storage, or config migration. Do not invent a different on-disk structure without a migration decision.

### Discovery

1. Nonempty `PSST_CONFIG`: return its value verbatim, regardless of existence. Relative paths are relative to caller cwd; no tilde/environment-variable expansion.
2. Starting at caller cwd, check **psst.json before .psst.json** at each level.
3. Walk ancestors through and including home directory. First match wins; configs are not merged.
4. If none found, use absolute `cwd/.psst.json`.

The chosen file's **directory** is provider context; not necessarily the caller directory. Shell commands use it as cwd; Terraform derives its cwd from it. Other providers ignore it. The wrapped command and `.env` export remain in caller cwd.

**Bug:** loop stops only when current directory equals home. If cwd is not inside home and no ancestor config exists, it reaches filesystem root and loops forever. Rust must terminate at root; decide whether then to check home separately. Use path components rather than string prefix for ancestor checks.

### Loading and saving

- Missing/unreadable/empty file returns empty config silently; an override pointing to a nonexistent file is allowed.
- Invalid JSON actually throws: `JSON.parse(file)` is evaluated before entering the async error wrapper. Do not trust the apparent catch intent.
- JSON null/false/zero become empty config; other invalid shapes are not validated and may break consumers.
- Opening console alone does not create a file. Save/add/rename/delete writes the entire object, two-space indentation, no trailing newline; creates parent directories recursively.
- Writes are synchronous/non-atomic; no locking, permission hardening, backup, concurrent-edit detection, or write error UI.
- Rename deletes old key and assigns new key; renaming to an existing name overwrites it without confirmation. Blank/arbitrary names accepted. Updates retain ordinary object order; renamed/new entries append (JS integer-like keys have numeric enumeration ordering).
- New name: highest key matching `^SECRET_(\d+)$` plus one, default `SECRET_1`; not first gap. New entry: manual source, empty value.

Rust: preserve deterministic entry order (not sorted HashMap traversal). Decide whether JS numeric-key ordering matters; conventional env names are insertion-ordered.

## 4. CLI, shell, and process contracts

| Invocation | Behavior |
| --- | --- |
| `psst` | Clear console and open TUI using chosen config. |
| `psst <command> [args...]` | Resolve config entries, overlay successful values on a copy of inherited environment, run direct child executable with original argv. |
| `psst --copy <secret>` | Resolve one exact environment-name entry and copy it; exit 0/1. |
| `psst --write` | Resolve entries and overwrite caller cwd `.env`; exit 0 on completed write. |
| `psst -h` / `psst --help` | Render interactive Help; pressing `h` exits 0. Does not automatically return after printing. |
| `psst -V` / `psst --version` | Commander version option, version from package metadata (0.7.3 here). |

No named subcommands, `--config`, list command, quiet/JSON mode, output filename, shell-hook/eval/export mode, or parent-shell environment mutation. `psst zsh` opens a child shell with secrets; it cannot modify the shell that launched psst. Shell expressions require explicitly invoking `sh -c` (e.g. `psst sh -c 'echo $A_SECRET'`).

Commander is configured with unknown options allowed and pass-through options: psst flags occur before command operands; downstream options must reach the wrapped command unconsumed, including downstream help/version flags. Preserve raw argv boundaries and test `--`, unknown leading flags, missing copy argument, command-local `--copy/--write/--help`, short flags, and mixed psst options against the old executable; these edge cases are not covered by current tests.

Dispatch priority: **copy → write → help → no-argument console → run**. Combination of copy/write/help does not trigger a mutual-exclusion error. Config discovery/loading occurs before all action modes, including help (version handled by parser separately).

### Injection

- Fetch sequentially in config enumeration order.
- Successful value overwrites same inherited environment key. Warning/exception/unknown provider leaves inherited key unchanged.
- Failures do not stop other fetches or prevent the wrapped command from launching.
- Shell/provider subprocesses inherit psst's original process environment, **not** the accumulating injected environment. Entries cannot depend on earlier injected secrets.
- Success counts even empty string if provider returns success (notably shell).
- Unknown warning: `Unknown source "<source>" for secret "<name>"`.
- Thrown-fetch warning: `Failed to fetch secret "<name>" from <source>`; discards underlying detail.
- Summary: `No secrets injected`, `injected 1 secret`, or `injected N secrets`.

Info uses green `\x1b[32m...\x1b[0m` on stdout; warnings use yellow `\x1b[33m⚠️  ...\x1b[0m` on stderr. Summary therefore contaminates stdout for pipelines even though secret data normally does not print. Decide explicitly whether a rewrite moves diagnostics to stderr/adds color detection.

### Wrapped child

Direct spawn (no implicit shell), inherited stdin/stdout/stderr, caller cwd, inherited environment with overlay. Wait for exit and mirror numeric exit status. Spawn error prints `Error: <message>` on stderr and exits 1. Currently signal termination maps missing numeric code to 0; there is no explicit signal forwarding, process-group handling, or exec replacement. Rust needs tests/decision for Ctrl-C, signals, job control, child shells, PATH overridden by config, and non-UTF8 OS arguments/environment.

### Clipboard

CLI errors: `Secret "<name>" not found`, `Unknown source "<source>"`, or `Failed to fetch secret "<name>"` on warning/empty success; exit 1. Successful message `📋 Secret <name> copied to clipboard`; exit 0. Catch reports exception message (or `Failed to copy secret`). Fetch only requested entry; no injection summary.

Current command is shell `echo "<secret>" | pbcopy`. Adds newline, is macOS-specific, interprets `$`, backticks, substitutions and embedded quotes, and can execute malicious secret contents. **Do not reproduce interpolation.** Feed bytes through clipboard process stdin; explicitly decide whether to retain/remove extra newline. Main-view copy has separate caching/warning behavior described below. No clipboard TTL/clear policy.

### .env export

Print `✏️  Creating .env file`; fetch all entries through Promise.all (concurrent async providers), retain config order in output. Unknown sources, warning results, and falsy/empty successful values are silently omitted. No injection summary or warning report. Uncaught provider exception/write failure aborts; not the same best-effort exception policy as injection.

Overwrite `.env` in **caller cwd**, not config directory. Output is exactly raw `KEY=value` joined by newline, no quoting/escaping/export prefix/final newline. Empty export writes empty file. Multiline, quote, whitespace, `#`, dollar-sign values may not round-trip in dotenv readers. Contains plaintext; file permissions follow normal write/umask, not explicitly 0600. Decide whether safer quoting/permissions are compatibility changes.

## 5. External-tool/provider contracts

All external tools rely on PATH and inherited authentication environment/session. There is no psst sign-in flow, HTTP client, or provider credential format. Current string-built provider commands go through the default system shell (on Unix `/bin/sh`, not `$SHELL`); Rust should normally use executable+argv for op/Vault/Terraform and shell evaluation **only** for shell-source commands.

### manual

- Identifier `manual`, reference label `value`.
- Return stored value unchanged, including whitespace; empty string warns `Secret [<key>] is empty`.
- Main safe display always eight bullets `••••••••` independent of length.
- Editor displays literal plaintext; append input/pasted chunks, Backspace/Delete removes final character, Left completes current value. Enter ignored. Hint advertises Ctrl+V but no special clipboard read is implemented.

### shell

- Identifier `shell`, reference label `shell command`; safe display is command itself (which can itself contain secrets).
- Execute full configured string through system shell in config directory; original psst environment, stdin piped, stdout captured UTF-8, stderr ignored.
- Trim both leading/trailing whitespace from stdout, not just a trailing newline. Empty stdout is successful; exit failure/spawn/cwd error warns `Failed to execute shell command for [<key>]`.
- Command may use pipelines, substitutions, scripts, relative paths and redirection. It is intentionally executable trusted configuration; can have side effects on any fetch, including opening/editing previews.
- Editor: append/delete command, Down fetches preview and displays output or warning, Left completes. Output is not automatically refreshed while typing.

### 1Password (`op`)

- Identifier `op`, reference label `1Password path`, safe display raw reference.
- Each fetch first executes `op --version`; any failure warns `1Password CLI is not installed` (even failure unrelated to installation).
- Then require nonempty `op://` prefix; warning `Invalid 1Password reference for [<key>]. Must start with 'op://'`.
- Execute `op read "<reference>"`, capture/trim stdout, suppress stderr. Empty warns `Secret [<key>] is empty`. Read failures throw to operation-level handling rather than returning warning.
- Version check uses caller cwd; no config cwd passed. No account/vault flags beyond the reference. No cache/batching across entries.

Browser subprocess API/JSON fields:

| Command | Consumed fields |
| --- | --- |
| `op vault list --format json` | array of `{id,name}` |
| `op item list --vault <vault-id> --format json` | array of `{id,title,vault:{id,name}}` |
| `op item get <item-id> --vault <item-vault-id> --format json` | `fields` array; each has `id,type,purpose,label,value,reference` |

Browser starts at vaults even for an existing reference. Right descends/selects field **reference supplied by op**, Left ascends; Left from vaults completes original value. Up/Down clamp. Case-insensitive substring search on item title, field label/purpose/type fallback; no vault filtering/search UI. Search resets on descent; returning upward may retain current query. Enter does not select. Five-row sliding window ending at selection after initial rows; total count and up/down indicators for longer lists. Vault/item/field emojis `🏰`, `📄`, `🔑`; selected yellow; field reference gray. Errors displayed: `Failed to fetch vaults. Make sure you're logged in to 1Password.`, `Failed to fetch items from vault.`, `Failed to fetch item fields.` Browser retrieves field values from CLI but only references/labels are rendered.

### 1Password 2FA (`op 2FA`)

Exact identifier **`op 2FA`**; same label, reference display, existence/reference/empty checks, and browser as op. Reads raw stored field using `op read`, not CLI OTP generation.

Parse returned text as URL. Protocol must be `otpauth:`; query `secret` must exist/nonempty. Generate local TOTP from base32 secret via speakeasy defaults (normally SHA-1, six digits, 30-second step, current Unix time; verify old library vectors for strict parity). URL host/type, issuer, label, `algorithm`, `digits`, `period`, counter are not used; even a HOTP URL with otpauth protocol gets TOTP treatment. Preserve leading zeroes in token. Bad URL/protocol/missing seed yields `Invalid otpauth URL in secret [<key>]: <error>`, with explicit `Invalid otpauth URL` / `Missing secret in otpauth URL` errors. No validity countdown or expiry invalidation; a revealed token remains cached until hidden/view remounted.

### HashiCorp Vault (`vault`)

- Identifier `vault`, reference label `Vault path`, safe display raw path.
- Value format `path#field`, commonly emitted as `/mount/path#field`. Split on `#`; only first two pieces used. Missing path/key throws `Invalid path format. Expected format: /path/path/path#secret`.
- Fetch `vault kv get -format=json <path>`; read **`.data.data[field]`** (KV v2 only). No generic secret engine/KV v1 support despite mount type filter only checking `kv`.
- Missing field: `Secret key '<field>' not found in path '<path>'`; command/parse/missing-key errors wrapped as `Failed to fetch secret: <message>`. Provider returns these errors as warnings.
- Object/array field values become compact JSON. Scalars (including number, bool, null, empty string) currently returned without proper conversion despite declared string type: consumer inconsistencies require a Rust decision. Fetch does not uniformly reject empty.
- No version preflight despite declared `--version` metadata. No psst-specific address/token/namespace args; Vault CLI handles environment/auth.

Browser integration:

- Mounts: `vault read -format=json sys/internal/ui/mounts`; use `.data.secret`, retain entries with `type == "kv"`, mount paths are map keys. Failure returns null; UI says `Failed to fetch KV mounts. Please ensure you're authenticated with Vault by running 'vault token lookup'.` No automatic lookup/login.
- At each path: attempt `vault kv list -format=json <path>` (array becomes folder entries), then `vault kv get -format=json <path>` (`Object.keys(.data.data)` become field entries). Each failure silently ignored independently; empty lists may mean permission/CLI errors.
- Folders listed first, fields second; case-insensitive search both. Right folder joins after stripping base trailing/segment leading slashes; Right field completes `/<currentPath>#<field>` (can emit double slash if currentPath ends with slash). Left uses slash components to parent, then mount picker at one-component root.
- Mount and path browsers use five-row windows, yellow selection, gray path metadata/counts, `💾` mounts, `📂` folders, `🔑` fields, loading labels, current path and Search labels.
- Left from mount picker completes **empty string**, not original reference; outer editor then considers it a draft value. Browsing existing value does not start at existing path.

### Terraform (experimental)

- Enabled by nonempty `PSST_EXPERIMENTAL`; identifier `terraform`, label `Terraform path`, raw safe display.
- Preflight `terraform --version`; failure warns `Terraform CLI is not installed`.
- Nonempty reference expected, formatted `directory#output_name`; empty warns `Invalid Terraform reference for [<key>].`.
- Split first directory/output pieces; cwd uses Node **path.join(configDirectory, directory)**, not generic path.resolve (absolute-looking second segments do not behave like Rust PathBuf::join replacement). Test path semantics before translating. `..` can leave config directory.
- Run `terraform output -json`, parse object, retrieve `json[output_name].value` regardless of Terraform sensitive metadata. Empty/falsy values (including 0/false/null) rejected; errors warn `Impossible to read secret from Terraform output [<reference>]`.
- Other values returned unconverted (objects/arrays/numbers can be consumer-invalid). No output caching or state setup. Editor is unfinished; decide whether to retain fetch-only experimental functionality and replace placeholder with a usable text editor.

## 6. User interface and persistence workflows

### Top-level views

Header `🤫 psst [<configFile>]`, path blue/underlined. Main/help/edit are mutually exclusive mounted views. Leaving main for help/edit discards main selection/cache/loading/warnings; returning starts selection 0 with secrets masked. No secrets fetched merely by rendering main table.

### Main view

| Key | Effect |
| --- | --- |
| `q` | Exit 0, even during delete confirmation. |
| Up / Down | Clamp selection, clear warnings; no wrap. |
| Right | Edit selected secret. |
| `a` | Add using generated name/manual default. |
| `d` | Stage selected name for deletion; clear warning; show red `Delete NAME? (y/n)`. |
| `y` / `n` while confirming | Delete/persist or cancel. All other inputs except q ignored during confirmation. |
| `p` | Reveal selected fetched secret or hide/remove its cached value. |
| `c` | Copy selected fetched secret using pbcopy. |
| `h` | Open help; h again returns. |

Table: rounded gray border/padding; headings **Name / Source / Value / Secret**; widths 20/15/40/40. Value uses provider safe display (unknown source masks); Secret shows eight bullets unless revealed. Value and secret truncated at 30 JS characters plus `...`; names/source not explicitly truncated. All rows rendered, no main-table scrolling/pagination. Selection yellow, pending deletion red, loading `loading...` yellow. Warning line yellow below table.

Reveal/copy add name to loading set, wait 100ms so UI can render loading, fetch, then remove loading state. Warning result shows provider warning; empty successful string shows `Secret [<key>] is empty`; thrown fetch shows `Failed to fetch from <source>`; unknown shows `Unknown source "<source>"`.

Reveal caches successful value by name; hide deletes it. Copy uses currently revealed cache if truthy, otherwise fetches without caching/revealing. Cached copy bypasses fetch and does not clear prior warnings; its subprocess call sits outside try/catch. Fetch-copy clears warnings on success. No success notification in TUI. No timers/re-fetch for expiring secrets. Delete last selected row moves selection up; deletion persists immediately.

### Secret editing form

Draft fields in order **name → source → value → secret**; starts at name. Up/Down bounded. Tab cycles provider only while Source is focused. Name accepts single-character input and trailing-character deletion; no env-name validation. Value is stored reference/literal; Secret is fetched preview. Both can display plaintext without masking.

- Left **or Enter** saves draft and returns to main (Enter is implemented but not advertised).
- Escape discards entire draft and returns.
- Right from value **or secret** opens current provider's editor.
- Nested provider completion changes draft value and returns to form, not disk. Save later persists.
- Preview automatically fetches on editor mount and source/value changes after 50ms; warnings/exceptions silently clear preview. Name-only changes do not trigger refetch. Opening a shell secret for editing therefore executes it automatically.
- Header in provider editor shows provider-specific hints; main form advertises Left save, Up/Down fields, Escape revert.
- Preview displays Loading... or fetched plaintext; errors not explained.

### Existing UI bugs / rewrite decisions

These are observable source behaviors but should not accidentally become requirements:

1. Main selection starts 0 even for empty config. `p/c/d/Right` destructure nonexistent entry and can crash; Down can set -1. Disable selected-item actions on empty lists.
2. Nested provider editor receives **original** secret, not draft value/current source. Reopening can reset edits; source-switch editors start with old provider's data. Pass current draft explicitly.
3. No universal Escape/Enter handling in provider editors. “Go back” generally completes rather than cancels. Terraform placeholder can trap user. Define nested cancellation without changing outer Escape behavior accidentally.
4. Vault mount input handler remains registered while nested path editor is active; both may process one key. Restrict input to active state/view.
5. Vault Right with no matching item can emit `#undefined` or navigate invalid data; search doesn't reset/clamp selection in path view. Empty/filtered list guards needed.
6. Vault path viewport item highlight arithmetic is inconsistent with mixed/scrolled folder+key rows.
7. Browser search branches append nonempty input before checking Backspace/Delete; actual terminal decoder behavior needs tests.
8. Concurrent preview/reveal requests can finish out of order and overwrite newer state; no cancellation or request identity. No in-flight action guard.
9. Revealed-value caching is truthiness-based rather than presence-based, and scalar provider outputs violate string assumptions.
10. Quit/raw-mode/Ctrl-C behavior partly delegated to Ink rather than application code. Rust must restore terminal state on all exits/errors/panics.

## 7. Rust implementation boundaries (suggested, not public API changes)

Use this decomposition to replicate workflows without coupling providers to a UI framework:

- `config`: discovery with finite ancestor walk, ordered serde JSON model, load diagnostics policy, persistence.
- `providers`: registry of exact identifier/metadata plus fetch behavior; shared typed `FetchResult { Success(String), Warning(String) }`.
- `process`: injectable subprocess executor supporting direct argv and intentional shell-string mode, cwd/env/stdin/stdout/stderr policies.
- `operations`: injection, single fetch/copy, env export; preserve distinct policies until deliberate unification.
- `ui`: explicit active-view state machine, editor draft, main selection/confirmation/loading/reveal state; provider browser states separate from fetch implementations.
- `clipboard`: platform service (macOS pbcopy baseline) with stdin byte transfer; optional Linux/Windows support clearly documented.
- `otp`: controllable clock and tested TOTP defaults.
- `cli`: pass-through-safe argument parser, composition root, exit/status/reporting policy.

Do not choose a map implementation that silently sorts entries. Keep stored references separate from fetched secrets. Preserve caller cwd versus config cwd. Use OS-native argv/env where appropriate; decide conversion policy for provider stdout, JSON scalars and non-UTF8 data. Avoid emitting secrets in error logs/debug representations. Async runtime/TUI crate selection is an implementation choice, not a compatibility requirement.

## 8. Acceptance tests and decisions before replacing the old tool

### Essential compatibility suite

- Discovery: override absolute/relative/missing/empty; both files (psst.json wins); nearest ancestor; home itself; fallback; outside-home/root termination.
- Config: valid ordered entries/all six identifiers; malformed JSON/unreadable/wrong shapes; unknown fields/source; create parents; CRUD/rename/overwrite semantics; no creation on console open.
- CLI: no args, each flag, priorities, version, copy missing operand, `--`, downstream help/version/unknown flags and quoted argv containing spaces/metacharacters.
- Injection: overwrite inherited key only on success, keep inherited key on failure, sequential ordering, partial/all failures, empty shell success, unknown source, provider exceptions, count and stdout/stderr routing.
- Shell context: execute relative script from config directory when caller differs; inherited auth vars; prove injected entries do not feed later shell fetches; stdout trimming and stderr/exit policy.
- Child: exact argv, stdin/stdout/stderr, caller cwd, environment overlay/PATH lookup, exit nonzero, missing executable, signals/Ctrl-C/interactive shell/job control.
- Copy: missing/unknown/empty/warning/exception, cache vs fresh UI copy, newline policy; secrets containing quotes, `$()`, backticks, newlines copied literally without execution.
- Export: caller `.env` overwrite, entry order, omission policy, concurrent fetch failure, empty file, escaping/multiline policy and secure permissions.
- op: version/read argv and auth inheritance, invalid/empty/failure paths; browser JSON hierarchy/search/back/reference selection using fake executable fixtures.
- OTP: known fixed-time vectors with leading zeroes, base32 handling, bad/missing otpauth secret, ignored URL options parity, expiry/cache decisions.
- Vault: KV mounts filtering, KV v2 JSON, folder-only/key-only/mixed lists, command errors, missing field, malformed reference, object/scalar/null conversion, navigation/search/empty lists.
- Terraform: feature gate including `0`, path joining/`..`, output JSON/sensitive values, false/zero/object policy, usable exit from editor.
- TUI: all keys, empty main list, masks/truncation/loading/colors, help return, source order, new-name generation, preview side effects, draft reopening/source changes, save/cancel and delete confirmation, terminal restoration.

Fake provider binaries on a temporary PATH can log argv/cwd/env and return fixture JSON without accessing real accounts. Use a fake clock for OTP and injected clipboard adapter. Test TUI logic as state transitions and add terminal snapshots/PTY end-to-end checks for key decoding and child lifecycle.

### Explicit policy choices (do not silently change during port)

- Fix infinite config search; define outside-home behavior.
- Define malformed-config error behavior and schema validation rather than perpetuating accidental casts.
- Remove shell interpolation for non-shell integrations and clipboard; configuration shell source remains intentionally executable.
- Decide diagnostic streams/color handling and noninteractive help (current help waits for h).
- Decide signal exit conventions/exec vs spawned child and forwarding.
- Decide literal bytes/newline in clipboard; dotenv quoting and 0600/atomic writes.
- Decide warning/exception/empty behavior consistency across operations.
- Decide Vault/Terraform scalar serialization and unsupported KV v1 handling.
- Fix draft/input/empty-list/async UI bugs; decide how much visual layout parity is required on narrow terminals.
- Decide Terraform scope (existing fetch works experimentally, browser does not).
- Resolve MIT LICENSE versus ISC package metadata before Rust distribution.

The baseline goal is the same `psst` command, same config discovery precedence and JSON reference format, same provider identifiers/tool integrations, same environment-wrapper semantics and keyboard workflows. Security fixes and incomplete/buggy behavior should be intentional, documented deviations—not accidental differences introduced by Rust/library defaults.
