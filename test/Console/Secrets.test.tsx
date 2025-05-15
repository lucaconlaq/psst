import { describe, it, expect } from "vitest";
import { render } from "ink-testing-library";
import { Secrets } from "../../src/Console/Secrets.js";
import { manualSource } from "../../src/sources/manual/manual.js";

describe("Secrets", () => {
	it("should show all secret names", () => {
		const config = {
			SECRET_1: {
				source: "manual" as const,
				value: "value1",
			},
			SECRET_2: {
				source: "manual" as const,
				value: "value2",
			},
		};

		const { lastFrame } = render(
			<Secrets
				config={config}
				selectedIndex={0}
				loadedSecrets={{}}
				loadingSecrets={new Set()}
				deletingName={null}
				sources={[manualSource]}
			/>,
		);

		expect(lastFrame()).toContain("SECRET_1");
		expect(lastFrame()).toContain("SECRET_2");
	});
});
