import { describe, it, expect, vi } from "vitest";
import { render } from "ink-testing-library";
import MainView from "../../src/Console/MainView.js";
import { manualSource } from "../../src/sources/manual/manual.js";

describe("MainView", () => {
	const TIMEOUT = 50; // Shorter timeout for faster tests

	const createTestConfig = () => ({
		SECRET_1: {
			source: "manual" as const,
			value: "value1",
		},
		SECRET_2: {
			source: "manual" as const,
			value: "value2",
		},
	});

	it("should delete a secret when pressing 'd' and confirming with 'y'", async () => {
		const config = createTestConfig();
		let deletedKey: string | null = null;
		const onDelete = (key: string) => {
			deletedKey = key;
		};

		const { stdin, lastFrame } = render(
			<MainView config={config} sources={[manualSource]} onEdit={() => {}} onDelete={onDelete} onHelp={() => {}} />,
		);

		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		expect(lastFrame()).toContain("SECRET_1");
		expect(lastFrame()).not.toContain("Delete SECRET_1?");

		stdin.write("d");
		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		expect(lastFrame()).toContain("Delete SECRET_1?");
		expect(deletedKey).toBeNull();

		stdin.write("y");
		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		expect(deletedKey).toBe("SECRET_1");
	});

	it("should not delete a secret when pressing 'd' and canceling with 'n'", async () => {
		const config = createTestConfig();
		let deletedKey: string | null = null;
		const onDelete = (key: string) => {
			deletedKey = key;
		};

		const { stdin, lastFrame } = render(
			<MainView config={config} sources={[manualSource]} onEdit={() => {}} onDelete={onDelete} onHelp={() => {}} />,
		);

		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		expect(lastFrame()).toContain("SECRET_1");
		expect(lastFrame()).not.toContain("Delete SECRET_1?");

		stdin.write("d");
		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		expect(lastFrame()).toContain("Delete SECRET_1?");
		expect(deletedKey).toBeNull();

		stdin.write("n");
		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		expect(lastFrame()).not.toContain("Delete SECRET_1?");
		expect(deletedKey).toBeNull();
	});

	it("should navigate through secrets and edit selected secret", async () => {
		const config = createTestConfig();
		let editKey: string | null = null;
		const onEdit = (key: string | null) => {
			editKey = key;
		};

		const { stdin, lastFrame } = render(
			<MainView config={config} sources={[manualSource]} onEdit={onEdit} onDelete={() => {}} onHelp={() => {}} />,
		);

		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		expect(lastFrame()).toContain("SECRET_1");
		expect(lastFrame()).toContain("SECRET_2");
		expect(editKey).toBeNull();

		stdin.write("\u001b[C"); // Right arrow
		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		expect(editKey).toBe("SECRET_1");
		editKey = null;

		// Navigate down to second secret
		stdin.write("\u001b[B"); // Down arrow
		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));

		// Press right arrow to edit second secret
		stdin.write("\u001b[C"); // Right arrow
		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		expect(editKey).toBe("SECRET_2");
		editKey = null; // Reset for next test

		// Navigate up to first secret
		stdin.write("\u001b[A"); // Up arrow
		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));

		// Press right arrow to edit first secret again
		stdin.write("\u001b[C"); // Right arrow
		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		expect(editKey).toBe("SECRET_1");
	});

	it("should toggle secret visibility with 'p' key", async () => {
		const config = createTestConfig();
		const { stdin, lastFrame } = render(
			<MainView config={config} sources={[manualSource]} onEdit={() => {}} onDelete={() => {}} onHelp={() => {}} />,
		);

		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		// Initial state: secret value should be masked
		expect(lastFrame()).toContain("••••••••");

		// Toggle visibility
		stdin.write("p");
		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		// First we should see loading state
		expect(lastFrame()).toContain("loading...");

		// Wait for loading to complete
		await new Promise((resolve) => setTimeout(resolve, 150)); // Wait longer than the 100ms in handleToggleSecret
		expect(lastFrame()).toContain("value1");

		// Toggle back
		stdin.write("p");
		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		expect(lastFrame()).toContain("••••••••");
	});

	it("should enter add mode with 'a' key", async () => {
		const config = createTestConfig();
		let editKey: string | null = null;
		const onEdit = (key: string | null) => {
			editKey = key;
		};

		const { stdin } = render(
			<MainView config={config} sources={[manualSource]} onEdit={onEdit} onDelete={() => {}} onHelp={() => {}} />,
		);

		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		stdin.write("a");
		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		expect(editKey).toBeNull();
	});

	it("should show help when pressing 'h'", async () => {
		const config = createTestConfig();
		const onHelp = vi.fn();
		const { stdin, lastFrame } = render(
			<MainView config={config} sources={[manualSource]} onEdit={() => {}} onDelete={() => {}} onHelp={onHelp} />,
		);

		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		stdin.write("h");
		await new Promise((resolve) => setTimeout(resolve, TIMEOUT));
		expect(onHelp).toHaveBeenCalled();
	});
});
