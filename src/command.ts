import { Command } from "commander";
import { readFileSync } from "node:fs";
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
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { findConfig, loadConfig } from "./config.js";
import { terraformSource } from "./sources/terraform/terraform.js";
import { clipboard } from "./clipboard.js";
import { op2faSource } from "./sources/op-2fa/op-2fa.js";
import { writeEnvFile } from "./writeEnvFile.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export const setupCommand = () => {
	const program = new Command();

	const packageJson = JSON.parse(readFileSync(join(__dirname, "../package.json"), "utf8"));
	const version = packageJson.version;

	type CommandOptions = {
		copy?: string;
		write?: boolean;
		help?: boolean;
	};

	program
		.name("psst")
		.description("A secret management tool")
		.argument("[command...]", "Command to run with secrets")
		.allowUnknownOption()
		.passThroughOptions()
		.helpOption(false)
		.addHelpCommand(false)
		.version(version)
		.option("--copy <secret>", "Copy a specific secret to clipboard")
		.option("--write", "Create an env file")
		.option("-h, --help", "Show help")
		.action(async (command: string[], options: CommandOptions) => {
			const sources = [opSource, op2faSource, vaultSource, shellSource, manualSource];

			if (process.env.PSST_EXPERIMENTAL) {
				sources.push(terraformSource);
			}

			const env: NodeJS.ProcessEnv = { ...process.env };
			const configFile = findConfig();
			const config = await loadConfig();

			const [cmd, ...args] = command;

			if (options.copy) {
				await clipboard(options.copy, config, sources, dirname(configFile));
			} else if (options.write) {
				await writeEnvFile(config, sources, dirname(configFile));
			} else if (options.help) {
				render(React.createElement(Help, { onBack: () => process.exit(0) }));
			} else if (command.length === 0) {
				console.clear();
				render(React.createElement(Console, { config, configFile, sources }));
			} else {
				const configPath = dirname(configFile);
				await injectSecrets(config, env, sources, configPath);
				await runCommand(cmd, args, env);
			}
		});

	return program;
};
