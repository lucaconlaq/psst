import { Command } from "commander";
import { readFileSync, existsSync } from "fs";
import { SecretsConfig } from "./types.js";
import Console from "./Console/Console.js";
import { opSource } from "./sources/op/op.js";
import { shellSource } from "./sources/shell/shell.js";
import { manualSource } from "./sources/manual/manual.js";
import { render } from "ink";
import React from "react";
import Help from "./Console/Help.js";
import { injectSecrets } from "./injection.js";
import { runCommand } from "./runCommand.js";
import { vaultSource } from "./sources/vault/vault.js";
import { fileURLToPath } from "url";
import { dirname, join } from "path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export const setupCommand = () => {
  const program = new Command();

  const packageJson = JSON.parse(
    readFileSync(join(__dirname, "../package.json"), "utf8")
  );
  const version = packageJson.version;

  program
    .name("psst")
    .description("A secret management tool")
    .argument("[command...]", "Command to run with secrets")
    .allowUnknownOption()
    .passThroughOptions()
    .helpOption(false)
    .addHelpCommand(false)
    .version(version)
    .action(async (command: string[]) => {
      const sources = [opSource, shellSource, manualSource];
      if (process.env.PSST_VAULT_ENABLED) {
        sources.push(vaultSource);
      }
      const env: NodeJS.ProcessEnv = { ...process.env };
      const configFile = process.env.PSST_CONFIG || ".psst.json";
      let config: SecretsConfig = {};
      if (existsSync(configFile)) {
        config = JSON.parse(readFileSync(configFile, "utf8"));
      }

      const [cmd, ...args] = command;

      if (command.length === 0) {
        render(React.createElement(Console, { config, configFile, sources }));
      } else if (cmd == "--help" || cmd == "-h") {
        render(React.createElement(Help, { onBack: () => process.exit(0) }));
      } else {
        await injectSecrets(config, env, sources);
        await runCommand(cmd, args, env);
      }
    });

  return program;
};
