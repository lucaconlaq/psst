# psst

<p align="center">
  <img src="./.github/logo.png" alt="psst logo" width="200" />
</p>

A secret management tool that helps you manage and inject secrets into your environment. Supports [1Password CLI](https://developer.1password.com/docs/cli/), [Vault](https://developer.hashicorp.com/vault), [Terraform](https://developer.hashicorp.com/terraform) outputs, shell commands, and manual values. Experimental `local-storage` adds SSH-agent-backed encrypted local values. All providers are available by default.

**2.0.0-alpha.4 is a complete rewrite in Rust** of the original npm-distributed JavaScript/TypeScript application. This is an alpha prerelease: development and testing will continue through alpha and beta prereleases before the stable **2.0.0** release.

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
[local-storage setup and safety](#experimental-local-storage).
This feature is **not release-ready or production-approved**: its
[verification record](docs/tickets/local-storage/07-experimental-release-readiness.md)
distinguishes fake-agent evidence from unmet platform, live 1Password, and
independent crypto/format gates. A `local-storage` resolution failure stops
wrapped-command launch and `.env` export; other providers still warn and omit
failed values.

### Run Commands with Secrets

Run any command with your secrets injected into its environment:

```bash
psst <any command>
```

Secrets are fetched concurrently, except that 1Password reads (`op` and `op 2FA`) share a queue to avoid racing authentication. Local-storage SSH-agent signing is also serialized, with operation-local deduplication of identical records. Authentication for `op` remains managed by the 1Password CLI; psst does not cache authentication tokens.

Status messages and diagnostics go to stderr, keeping command stdout clean. On a terminal, status messages are green, warnings yellow, and errors red. Set `NO_COLOR` to disable colors; redirected stderr and `TERM=dumb` also use plain text.

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

Example config:

```json
{
  "API_KEY": {
    "source": "op",
    "value": "op://vault/item/field"
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

### Experimental local storage

Local storage requires a compatible deterministic **Ed25519 SSH agent** selected
explicitly through `SSH_AUTH_SOCK`. The target integration is an Ed25519 SSH Key
item generated and kept inside 1Password, with its SSH agent enabled. psst neither
exports private keys nor fetches an encryption key through `op`; no SSH server or
uploaded public key is needed. Follow
[1Password setup](https://developer.1password.com/docs/ssh/get-started/) and
[eligible-key guidance](https://developer.1password.com/docs/ssh/agent/).
The referenced setup excludes Linux Flatpak/Snap agent installations; live
compatibility has not been verified on any platform.

Example socket selection—confirm that it is your intended endpoint first:

```sh
# Linux
export SSH_AUTH_SOCK="$HOME/.1password/agent.sock"
# macOS: use this instead
export SSH_AUTH_SOCK="$HOME/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock"
```

`IdentityAgent` in SSH configuration alone does not select psst's socket. psst
never discovers or starts an agent; a vendor-looking socket path is not proof of
key custody. Prefer per-operation confirmation and do not forward the agent to
untrusted processes. Approval frequency depends on agent settings. An authorized
process can retain decryption capability even after the agent locks.

Configuration uses source `local-storage` and reference `id` or
`id#SHA256:fingerprint`. Ciphertext JSON and locks live directly in `.psst/`
beside the selected configuration, including when using `PSST_CONFIG`.
Ciphertext can be committed, but IDs, public keys, fingerprints, lengths, and
record presence remain visible. Manual values, environments, clipboard contents,
and `.env` exports are still plaintext. The
[spec](docs/specs/01-local-storage.md) owns the behavioral contract; the
[architecture](docs/ARCHITECTURE.md#selected-v1-json-format) documents the format.

After a trusted checkout, inspect paths and set `.psst/` permissions to `0700`
and JSON/existing lock files to `0600`. Git does not preserve these private modes;
psst refuses insecure storage rather than repairing it. Do not follow symlinks or
recursively alter unrelated files. Older `.psst/local-storage/` files are not read
or automatically migrated: with psst stopped, move their JSON and lock files into
`.psst/`, preserving permissions and avoiding filename collisions.

Keep the SSH key recoverable while any current or historical ciphertext is
needed. Losing it loses decryptability; repository backups alone cannot recover
values. Rotate while the old key works: retrieve the value in a trusted
environment, create a record under the new key, update affected references, and
verify in a fresh operation before retiring the old mapping/key. Clear plaintext
clipboard/export artifacts. Rotation cannot revoke old Git history or captured
keys. There is no automatic orphan cleanup or cross-file rollback; inspect
partial-completion and durability errors before retrying. See
[architecture recovery outcomes](docs/ARCHITECTURE.md#writes-conflicts-and-partial-outcomes).

## Glossary

[docs/GLOSSARY.md](docs/GLOSSARY.md) defines the canonical domain vocabulary for code, documentation, and discussions. Keep definitions concise and update them when terminology is resolved.

## Adding providers

See [the architecture](docs/ARCHITECTURE.md) for provider implementation details.

## 🛠️ CI and releases

Every branch push and pull request runs formatting and Clippy checks, tests, and release builds for Linux and macOS on both x86_64 and ARM64. Linux release binaries use musl.

Pushing a version tag matching `Cargo.toml` (for example, `v2.0.0-alpha.4`) publishes a GitHub Release only after all checks, tests, and builds succeed. Releases include platform-specific `.tar.gz` archives and `SHA256SUMS`. Tags containing a prerelease suffix, such as `-alpha.1`, `-beta.1`, or `-rc.1`, are marked as prereleases rather than the latest stable release. This workflow does not publish to npm or crates.io.

## ⚠️ Limitations

Linux and macOS are supported. Windows is not currently supported.
