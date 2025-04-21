import { SecretFetchResult, SecretFetchResultType } from "../../types.js";
import { execSync } from "child_process";
import { OpEditor } from "./OpEditor.js";
import { SecretConfig, SecretSource } from "../../types.js";

export interface OpSource extends SecretSource {
  executable: string;
  versionCommand: string;
  readCommand: string;
}

export const opSource: OpSource = {
  name: "op",
  safeValue: (secret) => secret.value,
  referenceName: "1Password path",
  executable: "op",
  versionCommand: "--version",
  readCommand: "read",
  fetchSecret: async function (
    secret: SecretConfig,
    key: string
  ): Promise<SecretFetchResult> {
    try {
      execSync(`${this.executable} ${this.versionCommand}`, {
        encoding: "utf8",
      });
    } catch (error) {
      return {
        type: SecretFetchResultType.Warning,
        warning: `1Password CLI is not installed`,
      };
    }

    if (!secret.value || !secret.value.startsWith("op://")) {
      return {
        type: SecretFetchResultType.Warning,
        warning: `Invalid 1Password reference for [${key}]. Must start with 'op://'`,
      };
    }

    const value = execSync(
      `${this.executable} ${this.readCommand} "${secret.value}"`,
      {
        encoding: "utf8",
        stdio: ["pipe", "pipe", "ignore"],
      }
    ).trim();

    if (!value) {
      return {
        type: SecretFetchResultType.Warning,
        warning: `Secret [${key}] is empty`,
      };
    }

    return { type: SecretFetchResultType.Success, value };
  },
  editorOptions: [
    { key: "←", description: "go back" },
    { key: "→", description: "select" },
    { key: "↑↓", description: "navigate" },
    { key: "type", description: "search" },
  ],
  renderEditor: OpEditor,
};
