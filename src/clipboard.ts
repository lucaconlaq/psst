import { execSync } from "node:child_process";
import type { SecretSource } from "./types.js";
import type { SecretsConfig } from "./types.js";
import { warn, info } from "./console.js";

export const clipboard = async (
	secretName: string,
	config: SecretsConfig,
	sources: SecretSource[],
	configPath: string,
): Promise<void> => {
	const secret = config[secretName];
	if (!secret) {
		warn(`Secret "${secretName}" not found`);
		process.exit(1);
	}

	const source = sources.find((s) => s.name === secret.source);
	if (!source) {
		warn(`Unknown source "${secret.source}"`);
		process.exit(1);
	}

	try {
		const result = await source.fetchSecret(secret, secretName, configPath);
		if (result.type === "warning" || (result.type === "success" && result.value.length === 0)) {
			warn(`Failed to fetch secret "${secretName}"`);
			process.exit(1);
		}

		execSync(`echo "${result.value}" | pbcopy`);
		info(`📋 Secret ${secretName} copied to clipboard`);
		process.exit(0);
	} catch (error) {
		warn(error instanceof Error ? error.message : "Failed to copy secret");
		process.exit(1);
	}
};
