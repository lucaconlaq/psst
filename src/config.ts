import { existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { homedir } from "node:os";
import { tryCatch } from "./utils/tryCatch.js";
import { readFile } from "node:fs/promises";
import type { SecretsConfig } from "./types.js";

const CONFIG_FILENAMES = ["psst.json", ".psst.json"];

const findConfig = (): string => {
	// If PSST_CONFIG is set, use it
	if (process.env.PSST_CONFIG) {
		return process.env.PSST_CONFIG;
	}

	// Start from current directory
	let currentDir = process.cwd();
	const homeDir = homedir();

	// Keep going up until we find a config file or reach home directory
	while (currentDir !== homeDir) {
		for (const filename of CONFIG_FILENAMES) {
			const configPath = join(currentDir, filename);
			if (existsSync(configPath)) {
				return configPath;
			}
		}

		// Move up one directory
		currentDir = resolve(currentDir, "..");
	}

	// Check home directory as well
	for (const filename of CONFIG_FILENAMES) {
		const configPath = join(homeDir, filename);
		if (existsSync(configPath)) {
			return configPath;
		}
	}

	// If no config found, use default .psst.json in current directory
	return join(process.cwd(), ".psst.json");
};

const loadConfig = async (): Promise<SecretsConfig> => {
	const configFile = findConfig();
	const { data: file } = await tryCatch(readFile(configFile, "utf8"));

	if (!file) {
		return {};
	}

	const { data: json } = await tryCatch(JSON.parse(file));
	if (!json) {
		return {};
	}

	return json as SecretsConfig;
};

export { loadConfig, findConfig };
