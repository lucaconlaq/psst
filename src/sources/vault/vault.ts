import { SecretFetchResult, SecretFetchResultType } from "../../types.js";
import { execSync } from "child_process";
import { VaultEditor } from "./VaultEditor.js";
import { SecretConfig, SecretSource } from "../../types.js";
import { resourceUsage } from "process";
import { fetchSecretFromPath } from "./vaultFetchData.js";

export interface VaultSource extends SecretSource {
  executable: string;
  versionCommand: string;
  readCommand: string;
}

export const vaultSource: VaultSource = {
  name: "vault",
  safeValue: (secret) => secret.value,
  referenceName: "Vault path",
  executable: "vault",
  versionCommand: "--version",
  readCommand: "read",
  fetchSecret: async function (
    secret: SecretConfig,
    key: string
  ): Promise<SecretFetchResult> {
    try {
      const value = await fetchSecretFromPath(secret.value);
      return {
        type: SecretFetchResultType.Success,
        value,
      };
    } catch (error) {
      return {
        type: SecretFetchResultType.Warning,
        warning: error instanceof Error ? error.message : "Unknown error",
      };
    }
  },
  editorOptions: [
    { key: "←", description: "Go back" },
    { key: "→", description: "Select" },
    { key: "↑↓", description: "Navigate" },
    { key: "type", description: "Search" },
  ],
  renderEditor: VaultEditor,
};
