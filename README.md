# psst 🤫

A secret management tool that helps you manage and inject secrets into your environment.

![Project Demo](.github/demo.gif)

## Installation

```bash
npm install -g psst
```

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

### 1. Interactive Console

The easiest way to manage your secrets is through the interactive console. Just run `psst` and use the intuitive interface to add, edit, and delete secrets.

### 2. Configuration File

psst automatically creates and uses `.psst.json` in your project root. You can edit it directly:

```json
{
  "SECRET_NAME": {
    "source": "op",
    "value": "op://vault/item/field"
  }
}
```

### 3. Custom Config Location

Override the default config file location using the `PSST_CONFIG` environment variable:

```bash
PSST_CONFIG=./secrets.json psst
```

## License

MIT
