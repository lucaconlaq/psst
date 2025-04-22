# psst 🤫

A secret management tool that helps you manage and inject secrets into your environment. Supports [1Password CLI](https://developer.1password.com/docs/cli/), [Vault](https://developer.hashicorp.com/vault) and shell commands.

![Project Demo](.github/demo.gif)

## Installation

You can install psst in several ways:

### Using npm (global installation)

```bash
npm install -g @lucaconlaq/psst
```

### Using npx (no installation required)

```bash
npx @lucaconlaq/psst
```

### Using mise (recommended)

[mise](https://mise.jdx.dev) is a modern version manager that makes it easy to manage multiple versions of tools. To install psst with mise:

```bash
mise use npm:@lucaconlaq/psst
```

This is the recommended way to install psst as it provides better version management and isolation.

## Usage

### Interactive Console

Open the interactive console to manage your secrets:

```bash
psst
```

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

## Configuration

psst looks for a config file in this order:

1. Use `PSST_CONFIG` if it is set.
2. Look for `psst.json` or `.psst.json` in the following locations:
   - Current directory
   - Parent directories (up to and including the home directory)
3. If no configuration file is found, use `.psst.json` in the current directory.

If the file does not exist, it will be created when the first secret is added via the console.

Example config:

```json
{
  "API_KEY": {
    "source": "op",
    "value": "op://vault/item/field"
  }
}
```

## License

MIT
