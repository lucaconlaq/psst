import { type SecretSource, type SecretFetchResult, SecretFetchResultType } from "../../types.js";
import { ManualEditor } from "./ManualEditor.js";

export const manualSource: SecretSource = {
	name: "manual",
	referenceName: "value",
	safeValue: (secret) => "••••••••",
	editorOptions: [
		{ key: "←", description: "go back" },
		{ key: "ctrl+v", description: "to paste from clipboard" },
	],
	fetchSecret: async (secret, key, configPath): Promise<SecretFetchResult> => {
		if (!secret.value) {
			return {
				type: SecretFetchResultType.Warning,
				warning: `Secret [${key}] is empty`,
			};
		}
		return { type: SecretFetchResultType.Success, value: secret.value };
	},
	renderEditor: ManualEditor,
};
