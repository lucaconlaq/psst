import { describe, it, expect } from "vitest";
import { shellSource } from "../../../src/sources/shell/shell.js";
import { SecretFetchResultType, type SecretConfig } from "../../../src/types.js";

describe("shellSource", () => {
	describe("name and referenceName", () => {
		it("should have correct name", () => {
			expect(shellSource.name).toBe("shell");
		});

		it("should have correct referenceName", () => {
			expect(shellSource.referenceName).toBe("shell command");
		});
	});

	describe("safeValue", () => {
		it("should return the command as is", () => {
			const secret: SecretConfig = { value: "echo 1", source: "shell" };
			expect(shellSource.safeValue(secret)).toBe("echo 1");
		});
	});

	describe("fetchSecret", () => {
		it("should execute echo command and return 1", async () => {
			const secret: SecretConfig = { value: "echo 1", source: "shell" };
			const result = await shellSource.fetchSecret(secret, "test-key", ".");
			expect(result.type).toBe(SecretFetchResultType.Success);
			if (result.type === SecretFetchResultType.Success) {
				expect(result.value).toBe("1");
			}
		});

		it("should return warning for invalid command", async () => {
			const secret: SecretConfig = {
				value: "invalid-command-that-does-not-exist",
				source: "shell",
			};
			const result = await shellSource.fetchSecret(secret, "test-key", ".");
			expect(result.type).toBe(SecretFetchResultType.Warning);
			if (result.type === SecretFetchResultType.Warning) {
				expect(result.warning).toBe("Failed to execute shell command for [test-key]");
			}
		});
	});

	describe("renderEditor", () => {
		it("should export ShellEditor component", () => {
			expect(shellSource.renderEditor).toBeDefined();
			expect(typeof shellSource.renderEditor).toBe("function");
		});
	});
});
