import type { SecretsConfig, SecretSource } from "./types.js";
import { warn, info } from "./console.js";

export const injectSecret = async (
	env: NodeJS.ProcessEnv,
	name: string,
	secret: any,
	source: SecretSource,
	configPath: string,
): Promise<boolean> => {
	try {
		const result = await source.fetchSecret(secret, name, configPath);
		if (result.type === "success") {
			env[name] = result.value;
			return true;
		}
		warn(result.warning);
		return false;
	} catch (error) {
		warn(`Failed to fetch secret "${name}" from ${secret.source}`);
		return false;
	}
};

export const injectSecrets = async (
	config: SecretsConfig,
	env: NodeJS.ProcessEnv,
	sources: SecretSource[],
	configPath: string,
) => {
	let successCount = 0;

	const secrets = Object.entries(config);
	for (const [name, secret] of secrets) {
		const source = sources.find((s) => s.name === secret.source);
		if (!source) {
			warn(`Unknown source "${secret.source}" for secret "${name}"`);
			continue;
		}

		if (await injectSecret(env, name, secret, source, configPath)) {
			successCount += 1;
		}
	}

	if (successCount === 0) {
		info("No secrets injected");
	} else if (successCount === 1) {
		info("injected 1 secret");
	} else {
		info(`injected ${successCount} secrets`);
	}
};
