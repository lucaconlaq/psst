# psst

<p align="center">
  <img src="./.github/logo.png" alt="psst logo" width="200" />
</p>

A secret management tool that helps you manage and inject secrets into your environment.

Supports:

- [1Password CLI](https://developer.1password.com/docs/cli/) (`op`), including one-time passwords (`op 2FA`)
- [Vault](https://developer.hashicorp.com/vault) (`vault`)
- [Terraform](https://developer.hashicorp.com/terraform) outputs (`terraform`)
- Shell commands (`shell`)
- Manual values (`manual`, stored in plaintext)
- SSH-agent-backed encrypted local values (`local-storage`)

## 📦 Installation

Version 2 is a Rust rewrite. Installation instructions using [mise](https://mise.jdx.dev) are coming soon; npm installation applies to the original version.

## 🚀 Usage

Open the interactive console to manage your secrets:

```bash
psst
```

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

# Write configured managed files (prompts before replacing files)
psst --write-files
```

## ⚙️ Configuration

psst uses `PSST_CONFIG` when set. Otherwise, it looks for `psst.json` or `.psst.json` in the current directory and its parents, up to the home directory. `psst.json` takes priority when both exist.

If none is found, `psst.json` is created in the current directory when the first entry is saved via the console.

Example config:

```json
{
  "API_KEY": {
    "source": "op",
    "value": "op://vault/item/field"
  }
}
```

Managed files use `"type": "file"` and a destination path as the key. They are written only with `psst --write-files`, not when running commands.

## ⚠️ Limitations

Linux and macOS are supported; Windows is not. Managed files are subject to filesystem restrictions.
