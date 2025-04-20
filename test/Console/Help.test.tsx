import { describe, it, expect } from "vitest";
import { render } from "ink-testing-library";
import React from "react";
import Help from "../../src/Console/Help.js";

describe("Help", () => {
  it("should show psst 🤫", () => {
    const { lastFrame } = render(<Help onBack={() => {}} />);
    expect(lastFrame()).toContain("Usage");
  });
});
