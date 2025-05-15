import { type SecretSource, type SecretFetchResult, SecretFetchResultType } from "../../types.js";
import { execSync } from "node:child_process";
import { ShellEditor } from "./ShellEditor.js";

export const shellSource: SecretSource = {
	name: "shell",
	safeValue: (secret) => secret.value,
	referenceName: "shell command",
	fetchSecret: async (secret, key, configPath): Promise<SecretFetchResult> => {
		try {
			const value = execSync(secret.value, {
				cwd: configPath,
				encoding: "utf8",
				stdio: ["pipe", "pipe", "ignore"],
			}).trim();

			return { type: SecretFetchResultType.Success, value };
		} catch (error) {
			return {
				type: SecretFetchResultType.Warning,
				warning: `Failed to execute shell command for [${key}]`,
			};
		}
	},
	editorOptions: [
		{ key: "←", description: "go back" },
		{ key: "↓", description: "to check output" },
	],
	renderEditor: ShellEditor,
};
