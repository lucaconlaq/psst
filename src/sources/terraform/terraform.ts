import { SecretFetchResult, SecretFetchResultType } from "../../types.js";
import { execSync } from "child_process";
import { TerraformEditor } from "./TerraformEditor.js";
import { SecretConfig, SecretSource } from "../../types.js";

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
  readCommand: "output",
  fetchSecret: async function (
    secret: SecretConfig,
    key: string,
    configPath: string
  ): Promise<SecretFetchResult> {
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
    return {
      type: SecretFetchResultType.Success,
      value: "output",
    };
  },
  editorOptions: [{ key: "←", description: "go back" }],
  renderEditor: TerraformEditor,
};
