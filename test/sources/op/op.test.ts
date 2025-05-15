import { describe, it, expect } from "vitest";
import { opSource } from "../../../src/sources/op/op.js";
import { SecretFetchResultType, type SecretConfig } from "../../../src/types.js";

describe("opSource", () => {
	describe("name and referenceName", () => {
		it("should have correct name", () => {
			expect(opSource.name).toBe("op");
		});

		it("should have correct referenceName", () => {
			expect(opSource.referenceName).toBe("1Password path");
		});
	});

	describe("safeValue", () => {
		it("should return the path as is", () => {
			const secret: SecretConfig = {
				value: "op://vault/item/field",
				source: "op",
			};
			expect(opSource.safeValue(secret)).toBe("op://vault/item/field");
		});
	});

	describe("fetchSecret", () => {
		it("should execute echo command and return the full command", async () => {
			const secret: SecretConfig = {
				value: "op://vault/item/field",
				source: "op",
			};
			const testSource = { ...opSource, executable: "echo" };
			const result = await testSource.fetchSecret(secret, "test-key", ".");
			expect(result.type).toBe(SecretFetchResultType.Success);
			if (result.type === SecretFetchResultType.Success) {
				expect(result.value).toBe("read op://vault/item/field");
			}
		});

		it("should return warning for invalid command", async () => {
			const secret: SecretConfig = {
				value: "op://vault/item/field",
				source: "op",
			};
			const testSource = { ...opSource, executable: "invalid-command" };
			const result = await testSource.fetchSecret(secret, "test-key", ".");
			expect(result.type).toBe(SecretFetchResultType.Warning);
			if (result.type === SecretFetchResultType.Warning) {
				expect(result.warning).toBe("1Password CLI is not installed");
			}
		});

		it("should return warning for invalid path", async () => {
			const secret: SecretConfig = {
				value: "invalid-path",
				source: "op",
			};
			const testSource = { ...opSource, executable: "echo" };
			const result = await testSource.fetchSecret(secret, "test-key", ".");
			expect(result.type).toBe(SecretFetchResultType.Warning);
			if (result.type === SecretFetchResultType.Warning) {
				expect(result.warning).toBe("Invalid 1Password reference for [test-key]. Must start with 'op://'");
			}
		});
	});

	describe("renderEditor", () => {
		it("should export OpEditor component", () => {
			expect(opSource.renderEditor).toBeDefined();
			expect(typeof opSource.renderEditor).toBe("function");
		});
	});
});
