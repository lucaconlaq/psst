import { describe, it, expect } from "vitest";
import { injectSecret, injectSecrets } from "../src/injection.js";
import { opSource } from "../src/sources/op/op.js";
import { shellSource } from "../src/sources/shell/shell.js";
import { manualSource } from "../src/sources/manual/manual.js";

const testOpSource = {
	...opSource,
	executable: "echo",
	versionCommand: "1.0.0",
	readCommand: "",
};

describe("injectSecret", () => {
	it("should inject a secret from 1Password", async () => {
		const env: NodeJS.ProcessEnv = {};
		const result = await injectSecret(
			env,
			"SECRET",
			{
				source: "op",
				value: "op://vault/item/field",
			},
			testOpSource,
			".",
		);
		expect(result).toBe(true);
		expect(env.SECRET).toBe("op://vault/item/field");
	});

	it("should inject a secret from shell", async () => {
		const env: NodeJS.ProcessEnv = {};
		const result = await injectSecret(
			env,
			"SECRET",
			{
				source: "shell",
				value: "echo 'shell-secret'",
			},
			shellSource,
			".",
		);
		expect(result).toBe(true);
		expect(env.SECRET).toBe("shell-secret");
	});

	it("should inject a manual secret", async () => {
		const env: NodeJS.ProcessEnv = {};
		const result = await injectSecret(
			env,
			"SECRET",
			{
				source: "manual",
				value: "manual-secret",
			},
			manualSource,
			".",
		);
		expect(result).toBe(true);
		expect(env.SECRET).toBe("manual-secret");
	});

	it("should handle failed secret injection", async () => {
		const env: NodeJS.ProcessEnv = {};
		const result = await injectSecret(
			env,
			"SECRET",
			{
				source: "op",
				value: "invalid-op-reference",
			},
			testOpSource,
			".",
		);
		expect(result).toBe(false);
		expect(env.SECRET).toBeUndefined();
	});
});

describe("injectSecrets", () => {
	it("should inject multiple secrets", async () => {
		const env: NodeJS.ProcessEnv = {};
		const config = {
			OP_SECRET: {
				source: "op" as const,
				value: "op://vault/item/field",
			},
			SHELL_SECRET: {
				source: "shell" as const,
				value: "echo 'shell-secret'",
			},
			MANUAL_SECRET: {
				source: "manual" as const,
				value: "manual-secret",
			},
		};

		await injectSecrets(config, env, [testOpSource, shellSource, manualSource], ".");

		expect(env.OP_SECRET).toBe("op://vault/item/field");
		expect(env.SHELL_SECRET).toBe("shell-secret");
		expect(env.MANUAL_SECRET).toBe("manual-secret");
	});

	it("should handle empty config", async () => {
		const env: NodeJS.ProcessEnv = {};
		await injectSecrets({}, env, [testOpSource, shellSource, manualSource], ".");
		expect(Object.keys(env)).toHaveLength(0);
	});

	it("should handle partial failures", async () => {
		const env: NodeJS.ProcessEnv = {};
		const config = {
			GOOD_SECRET: {
				source: "manual" as const,
				value: "good-secret",
			},
			BAD_SECRET: {
				source: "op" as const,
				value: "invalid-op-reference",
			},
		};

		await injectSecrets(config, env, [testOpSource, shellSource, manualSource], ".");

		expect(env.GOOD_SECRET).toBe("good-secret");
		expect(env.BAD_SECRET).toBeUndefined();
	});
});
