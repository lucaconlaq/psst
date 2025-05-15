import { type SecretFetchResult, SecretFetchResultType } from "../../types.js";
import { execSync } from "child_process";
import { TerraformEditor } from "./TerraformEditor.js";
import type { SecretConfig, SecretSource } from "../../types.js";
import { join } from "path";

export interface TerraformSource extends SecretSource {
	executable: string;
	versionCommand: string;
	readCommand: string;
}

export const terraformSource: TerraformSource = {
	name: "terraform",
	safeValue: (secret) => secret.value,
	referenceName: "Terraform path",
	executable: "terraform",
	versionCommand: "--version",
	readCommand: "output -json",
	fetchSecret: async function (secret: SecretConfig, key: string, configPath: string): Promise<SecretFetchResult> {
		try {
			execSync(`${this.executable} ${this.versionCommand}`, {
				encoding: "utf8",
			});
		} catch (error) {
			return {
				type: SecretFetchResultType.Warning,
				warning: `Terraform CLI is not installed`,
			};
		}

		if (!secret.value) {
			return {
				type: SecretFetchResultType.Warning,
				warning: `Invalid Terraform reference for [${key}].`,
			};
		}

		try {
			const terraformPath = secret.value.split("#")[0];
			const terraformItem = secret.value.split("#")[1];
			const command = `${this.executable} ${this.readCommand}`;
			const fullPath = join(configPath, terraformPath);

			const result = execSync(command, {
				encoding: "utf8",
				cwd: fullPath,
			}).trim();

			const json = JSON.parse(result);
			const value = json[terraformItem]["value"];

			if (!value) {
				return {
					type: SecretFetchResultType.Warning,
					warning: `Impossible to read secret from Terraform output [${secret.value}]`,
				};
			}
			return { type: SecretFetchResultType.Success, value };
		} catch (error) {
			return {
				type: SecretFetchResultType.Warning,
				warning: `Impossible to read secret from Terraform output [${secret.value}]`,
			};
		}
	},
	editorOptions: [{ key: "←", description: "go back" }],
	renderEditor: TerraformEditor,
};
