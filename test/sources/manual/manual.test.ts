import { describe, it, expect } from "vitest";
import { manualSource } from "../../../src/sources/manual/manual.js";
import { SecretFetchResultType, SecretConfig } from "../../../src/types.js";

describe("manualSource", () => {
  describe("name and referenceName", () => {
    it("should have correct name", () => {
      expect(manualSource.name).toBe("manual");
    });

    it("should have correct referenceName", () => {
      expect(manualSource.referenceName).toBe("value");
    });
  });

  describe("safeValue", () => {
    it("should return masked value", () => {
      const secret: SecretConfig = { value: "secret123", source: "manual" };
      expect(manualSource.safeValue(secret)).toBe("••••••••");
    });

    it("should return same masked value regardless of input length", () => {
      const shortSecret: SecretConfig = { value: "a", source: "manual" };
      const longSecret: SecretConfig = {
        value: "verylongsecret",
        source: "manual",
      };
      expect(manualSource.safeValue(shortSecret)).toBe("••••••••");
      expect(manualSource.safeValue(longSecret)).toBe("••••••••");
    });
  });

  describe("fetchSecret", () => {
    it("should return warning for empty secret", async () => {
      const secret: SecretConfig = { value: "", source: "manual" };
      const result = await manualSource.fetchSecret(secret, "test-key", ".");
      expect(result.type).toBe(SecretFetchResultType.Warning);
      if (result.type === SecretFetchResultType.Warning) {
        expect(result.warning).toBe("Secret [test-key] is empty");
      }
    });

    it("should return success with value for non-empty secret", async () => {
      const secret: SecretConfig = { value: "secret123", source: "manual" };
      const result = await manualSource.fetchSecret(secret, "test-key", ".");
      expect(result.type).toBe(SecretFetchResultType.Success);
      if (result.type === SecretFetchResultType.Success) {
        expect(result.value).toBe("secret123");
      }
    });
  });

  describe("renderEditor", () => {
    it("should export ManualEditor component", () => {
      expect(manualSource.renderEditor).toBeDefined();
      expect(typeof manualSource.renderEditor).toBe("function");
    });
  });
});
