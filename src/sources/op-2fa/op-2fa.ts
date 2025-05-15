import { type SecretFetchResult, SecretFetchResultType } from "../../types.js";
import { execSync } from "node:child_process";
import { OpEditor } from "../op/OpEditor.js";
import type { SecretConfig, SecretSource } from "../../types.js";
import { totp } from "otplib";

export interface OpSource extends SecretSource {
	executable: string;
	versionCommand: string;
	readCommand: string;
}

export const op2faSource: OpSource = {
	name: "op 2FA",
	safeValue: (secret) => secret.value,
	referenceName: "1Password path",
	executable: "op",
	versionCommand: "--version",
	readCommand: "read",
	fetchSecret: async function (secret: SecretConfig, key: string, configPath: string): Promise<SecretFetchResult> {
		try {
			execSync(`${this.executable} ${this.versionCommand}`, {
				encoding: "utf8",
			});
		} catch (error) {
			return {
				type: SecretFetchResultType.Warning,
				warning: "1Password CLI is not installed",
			};
		}

		if (!secret.value || !secret.value.startsWith("op://")) {
			return {
				type: SecretFetchResultType.Warning,
				warning: `Invalid 1Password reference for [${key}]. Must start with 'op://'`,
			};
		}

		const value = execSync(`${this.executable} ${this.readCommand} "${secret.value}"`, {
			encoding: "utf8",
			stdio: ["pipe", "pipe", "ignore"],
		}).trim();

		if (!value) {
			return {
				type: SecretFetchResultType.Warning,
				warning: `Secret [${key}] is empty`,
			};
		}

		try {
			const url = new URL(value);
			if (url.protocol !== "otpauth:") {
				throw new Error("Invalid otpauth URL");
			}

			const secret = url.searchParams.get("secret");
			if (!secret) {
				throw new Error("Missing secret in otpauth URL");
			}

			const token = totp.generate(secret);

			return {
				type: SecretFetchResultType.Success,
				value: token,
			};
		} catch (err) {
			return {
				type: SecretFetchResultType.Warning,
				warning: `Invalid otpauth URL in secret [${key}]: ${(err as Error).message}`,
			};
		}
	},
	editorOptions: [
		{ key: "←", description: "go back" },
		{ key: "→", description: "select" },
		{ key: "↑↓", description: "navigate" },
		{ key: "type", description: "search" },
	],
	renderEditor: OpEditor,
};
