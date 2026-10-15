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

## ⚠️ Limitations

Linux and macOS are supported. Windows is not currently supported.
