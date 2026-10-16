# psst

<p align="center">
  <img src="./.github/logo.png" alt="psst logo" width="200" />
</p>

A secret management tool that helps you manage and inject secrets into your environment. Supports [1Password CLI](https://developer.1password.com/docs/cli/), [Vault](https://developer.hashicorp.com/vault), [Terraform](https://developer.hashicorp.com/terraform) outputs, shell commands, and manual values. Experimental `local-storage` adds SSH-agent-backed encrypted local values. All providers are available by default.

**2.0.0 is a complete rewrite in Rust** of the original npm-distributed JavaScript/TypeScript application.

![Project Demo](.github/demo.gif)

## 📦 Installation

Installation instructions for the Rust rewrite using [mise](https://mise.jdx.dev) are coming soon.

## 🚀 Usage

### Interactive Console

Open the interactive console to manage your secrets:

```bash
psst
```

The console explains its controls in-place. For encrypted local values, see
[the local-storage spec](docs/specs/01-local-storage.md).
Implementation completion does not establish production approval; the
[spec](docs/specs/01-local-storage.md#release-gates) defines the release gates.
A `local-storage` resolution failure stops wrapped-command launch and `.env` export; other providers still warn and omit
failed values.

### Run Commands with Secrets

Run any command with your secrets injected into its environment:

```bash
psst <any command>
```

### Examples

```bash
# Open a shell with secrets
psst zsh

# Run a development server
psst npm run dev

# View injected secrets
psst env

# Use secrets in a shell command
psst sh -c 'echo $A_SECRET'
```

## ⚙️ Configuration

psst looks for a config file in this order:

1. Use `PSST_CONFIG` if it is set.
2. Look for `psst.json` or `.psst.json` in the following locations:
   - Current directory
   - Parent directories (up to and including the home directory)
3. If no configuration file is found, use `psst.json` in the current directory.

Existing `.psst.json` files remain supported; `psst.json` takes priority when both exist in the same directory.

If the file does not exist, it will be created when the first entry is saved via the console. Creating an encrypted local value alone does not save configuration.

Each mapping may include `"type": "env"` (the default when omitted) or
`"type": "file"`. Saves emit explicit types. File mappings use their literal
destination path as the key. Dashboard Tab switches Entries/Files, keeping each
view's filter and selection. Files shows Destination, Source, Reference, and Contents, with a selectable
`+ New file` row. Ctrl+N or Enter on that row creates a draft; Enter on a mapping
edits it. Tab/Shift+Tab moves between editor fields, Left/Right chooses the source,
and Ctrl+B browses supported providers. Ctrl+S saves the mapping, Esc discards
the draft, and Ctrl+D confirms mapping deletion. Type filters the dashboard;
Backspace erases the filter, F1 opens help, and Esc or Ctrl+C quits.
Saving, renaming, or deleting file mappings never writes or removes destination files
or provider contents. Contents stay masked; Ctrl+R reveals and Ctrl+Y copies from
the provider only on request, never from the destination.

File mappings are excluded from wrapped-command injection, `--copy NAME` lookup,
and `--write` dotenv export. Materialize all configured files explicitly:

```bash
psst --write-files          # One Replace all? [y/N] prompt for differing existing files
psst --write-files --yes    # Approve replacements noninteractively; safety still applies
```

Relative destinations resolve beside the configuration, not the caller's working
directory. No destination writes (even new files) occur before required approval.
Refusal or missing noninteractive consent cancels the batch. After approval,
files succeed independently: reports show destination paths and `written`,
`unchanged`, or `failed`, never contents; any failure exits unsuccessfully.
New parents are 0700 and written files are current-user 0600 atomic replacements;
existing directory modes stay unchanged. Symlinks and unsafe targets are refused.
Only identical files already at 0600 are unchanged; other identical files fail
rather than silently repairing permissions. Completed writes survive other errors
or cancellation; a post-replacement sync failure means uncertain durability, not
rollback. An empty file set succeeds with guidance to Files.

Filesystem name equivalence must be established read-only before any provider
fetch; otherwise the entire run is refused, including with `--yes`. Currently,
Linux supports only ext-family directories whose flags can be read and show no
casefold; other filesystems (including overlay/network mounts) and casefold
directories are refused. macOS requires known volume case sensitivity and ASCII
path components; non-ASCII paths are refused even on case-sensitive volumes
because Unicode normalization equivalence is not established. Missing parents
inherit checked ancestor semantics; existing mount points are checked separately.
No destination probe files are created.

Destinations and temporary staging contain plaintext. Normal exit and cancellation
clean unfinished temporary files, but crashes can leave staging behind in the
system temporary directory (`psst-files-*`); inspect and remove leftovers manually.
Absolute destinations and paths outside the configuration directory are allowed:
this is not a sandbox. Only use trusted shell-provider commands: requested
resolution executes them **before** replacement consent, so refusal prevents psst
destination writes, not provider side effects. Ctrl+C exits unsuccessfully and
cleans unfinished temporary files, but retains completed writes. There is no
all-files transaction or rollback. Replacements do not preserve previous modes,
ownership, ACLs, or extended attributes.

Resolved UTF-8 contents are limited to 4 MiB per file (providers may impose stricter
limits). psst adds no trimming, newline, interpolation, or other transformation;
existing provider normalization and empty-value rules still apply. `~` and
environment variables in destination paths are literal, not expanded.

Example config:

```json
{
  "API_KEY": {
    "type": "env",
    "source": "op",
    "value": "op://vault/item/field"
  },
  "./config.local": {
    "type": "file",
    "source": "op",
    "value": "op://vault/item/config"
  }
}
```

Terraform references use `directory#output_name`, with the directory relative to the config file. For example:

```json
{
  "TF_SECRET": {
    "source": "terraform",
    "value": "terraform-directory#output_name"
  }
}
```

psst runs `terraform output -json` in that directory and selects the named output.

## ⚠️ Limitations

Linux and macOS are supported, subject to the filesystem restrictions above.
Windows is not currently supported.

Managed files do not support reverse sync/upload, templating/interpolation,
binary attachments or dedicated downloads, automatic materialization or destination
cleanup, subsets, dry-run/watch/JSON modes, local-content viewing/comparison,
automatic crash recovery, or preservation of old destination metadata.

`local-storage` remains experimental: managed-file support does not grant it
production approval. Independent crypto/format review and Linux live compatibility
checks remain outstanding; prior consented macOS checks are scoped functional
evidence only. Agent authorization settings, durable decryption capability,
key-loss/recovery, and other limitations remain governed by the
[local-storage release gates](docs/specs/01-local-storage.md#release-gates) and
[security constraints](docs/specs/01-local-storage.md#further-notes). Normal tests
use disposable fixtures and fake providers/agents, not real authentication or
personal secrets.
