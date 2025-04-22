import { describe, it, beforeEach, afterEach, expect } from "vitest";
import mock from "mock-fs";
import { findConfig } from "../src/config.js";

describe("findConfig", () => {
  const mockHomeDir = "/home/user";
  const mockCwd = "/home/user/projects/my-project";

  beforeEach(() => {
    process.cwd = () => mockCwd;
    process.env.HOME = mockHomeDir;
  });

  afterEach(() => {
    delete process.env.PSST_CONFIG;
    delete process.env.HOME;
    mock.restore();
  });

  it("uses PSST_CONFIG if set", () => {
    process.env.PSST_CONFIG = "/custom/path/config.json";
    const result = findConfig();
    expect(result).toBe("/custom/path/config.json");
  });

  it("finds .psst.json in current directory", () => {
    mock({
      "/home/user/projects/my-project/.psst.json": "{}",
    });

    const result = findConfig();
    expect(result).toBe("/home/user/projects/my-project/.psst.json");
  });

  it("finds psst.json in current directory", () => {
    mock({
      "/home/user/projects/my-project/psst.json": "{}",
    });

    const result = findConfig();
    expect(result).toBe("/home/user/projects/my-project/psst.json");
  });

  it("prefers .psst.json over psst.json in same directory", () => {
    mock({
      "/home/user/projects/my-project/.psst.json": "{}",
      "/home/user/projects/my-project/psst.json": "{}",
    });

    const result = findConfig();
    expect(result).toBe("/home/user/projects/my-project/psst.json");
  });

  it("finds config in parent directory", () => {
    mock({
      "/home/user/projects/.psst.json": "{}",
    });

    const result = findConfig();
    expect(result).toBe("/home/user/projects/.psst.json");
  });

  it("finds config in home directory", () => {
    mock({
      "/home/user/.psst.json": "{}",
    });

    const result = findConfig();
    expect(result).toBe("/home/user/.psst.json");
  });

  it("defaults to .psst.json in current directory if no config found", () => {
    mock({});

    const result = findConfig();
    expect(result).toBe("/home/user/projects/my-project/.psst.json");
  });

  it("finds first config file in search order", () => {
    mock({
      "/home/user/projects/my-project/.psst.json": "{}",
      "/home/user/projects/psst.json": "{}",
      "/home/user/.psst.json": "{}",
    });

    const result = findConfig();
    expect(result).toBe("/home/user/projects/my-project/.psst.json");
  });

  it("finds config in parent directory when not in current", () => {
    mock({
      "/home/user/projects/.psst.json": "{}",
    });

    const result = findConfig();
    expect(result).toBe("/home/user/projects/.psst.json");
  });

  it("finds config in home directory when not in current or parent", () => {
    mock({
      "/home/user/.psst.json": "{}",
    });

    const result = findConfig();
    expect(result).toBe("/home/user/.psst.json");
  });
});
