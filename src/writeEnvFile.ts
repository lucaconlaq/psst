import type { SecretSource } from "./types.js";
import type { SecretsConfig } from "./types.js";
import { info } from "./console.js";
import { writeFileSync } from "node:fs";

const writeEnvFile = async (config: SecretsConfig, sources: SecretSource[], configPath: string): Promise<void> => {
	info("✏️  Creating .env file");
	const envContent = await Promise.all(
		Object.entries(config).map(async ([key, secret]) => {
			const source = sources.find((s) => s.name === secret.source);
			if (!source) {
				return null;
			}

			const result = await source.fetchSecret(secret, key, configPath);
			if (result.type === "success" && result.value) {
				return `${key}=${result.value}`;
			}
			return null;
		}),
	);

	const validContent = envContent.filter((line): line is string => line !== null).join("\n");
	writeFileSync(".env", validContent);
	process.exit(0);
};

export { writeEnvFile };
