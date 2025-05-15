import { Box, Text, useInput } from "ink";
import { useState } from "react";
import { Secrets } from "./Secrets.js";
import type { SecretsConfig, SecretSource, Env } from "../types.js";

interface MainViewProps {
	config: SecretsConfig;
	configPath: string;
	sources: SecretSource[];
	onEdit: (key: string | null) => void;
	onDelete: (key: string) => void;
	onHelp: () => void;
}

const MainView = ({ config, configPath, sources, onEdit, onDelete, onHelp }: MainViewProps) => {
	// State management
	const [index, setIndex] = useState(0);
	const [toDelete, setToDelete] = useState<string | null>(null);
	const [loadedSecrets, setLoadedSecrets] = useState<Env>({});
	const [loadingSecrets, setLoadingSecrets] = useState<Set<string>>(new Set());
	const [warnings, setWarnings] = useState<Record<string, string>>({});

	const configEntries = Object.entries(config);

	const handleToggleSecret = async (): Promise<void> => {
		if (index >= 0) {
			const [secretName, secret] = configEntries[index];
			const source = sources.find((s) => s.name === secret.source);

			if (!source) {
				setWarnings((prev) => ({
					...prev,
					[secretName]: `Unknown source "${secret.source}"`,
				}));
				return;
			}

			if (loadedSecrets[secretName]) {
				const newSecrets = { ...loadedSecrets };
				delete newSecrets[secretName];
				setLoadedSecrets(newSecrets);
				setWarnings((prev) => {
					const newWarnings = { ...prev };
					delete newWarnings[secretName];
					return newWarnings;
				});
				return;
			}

			try {
				setLoadingSecrets((prev) => new Set(prev).add(secretName));
				const key = configEntries[index][0];
				await new Promise((resolve) => setTimeout(resolve, 100));
				const result = await source.fetchSecret(secret, key, configPath);

				if (result.type === "warning") {
					setWarnings((prev) => ({
						...prev,
						[secretName]: result.warning,
					}));
					return;
				}
				if (result.type === "success" && result.value.length === 0) {
					setWarnings((prev) => ({
						...prev,
						[secretName]: `Secret [${key}] is empty`,
					}));
					return;
				}
				setLoadedSecrets({ ...loadedSecrets, [secretName]: result.value });
				setWarnings((prev) => {
					const newWarnings = { ...prev };
					delete newWarnings[secretName];
					return newWarnings;
				});
			} catch (error) {
				setWarnings((prev) => ({
					...prev,
					[secretName]: `Failed to fetch from ${secret.source}`,
				}));
			} finally {
				setLoadingSecrets((prev) => {
					const next = new Set(prev);
					next.delete(secretName);
					return next;
				});
			}
		}
	};

	const handleAddSecret = (): void => {
		onEdit(null);
	};

	const handleEditSecret = (): void => {
		if (index >= 0) {
			const [secretName] = configEntries[index];
			onEdit(secretName);
		}
	};

	const handleDeleteSecret = (): void => {
		if (index >= 0) {
			const [secretName] = configEntries[index];
			setToDelete(secretName);
			setWarnings({});
		}
	};

	const handleConfirmDelete = (): void => {
		if (toDelete) {
			onDelete(toDelete);
			setToDelete(null);
			// Reset selection if we deleted the last item
			if (index >= configEntries.length - 1) {
				setIndex(Math.max(0, configEntries.length - 2));
			}
		}
	};

	const handleCancelDelete = (): void => {
		setToDelete(null);
	};

	const handleNavigateUp = (): void => {
		setIndex(Math.max(0, index - 1));
		setWarnings({});
		setToDelete(null);
	};

	const handleNavigateDown = (): void => {
		setIndex(Math.min(configEntries.length - 1, index + 1));
		setWarnings({});
		setToDelete(null);
	};

	useInput((input, key) => {
		if (input === "q") {
			process.exit(0);
			return;
		}

		if (toDelete) {
			if (input === "y") {
				handleConfirmDelete();
			} else if (input === "n") {
				handleCancelDelete();
			}
			return;
		}

		switch (input) {
			case "p":
				handleToggleSecret();
				break;
			case "h":
				onHelp();
				break;
			case "a":
				handleAddSecret();
				break;
			case "d":
				handleDeleteSecret();
				break;
		}

		if (key.rightArrow) {
			handleEditSecret();
		}

		if (key.upArrow) {
			handleNavigateUp();
		} else if (key.downArrow) {
			handleNavigateDown();
		}
	});

	return (
		<Box flexDirection="column">
			<Box marginBottom={1}>
				<Text>
					<Text>Press </Text>
					<Text color="yellow">q</Text>
					<Text> to quit, </Text>
					<Text color="yellow">p</Text>
					<Text> to toggle selected secret, </Text>
					<Text color="yellow">↑↓</Text>
					<Text> to navigate, </Text>
					<Text color="yellow">→</Text>
					<Text> to edit, </Text>
					<Text color="yellow">d</Text>
					<Text> to delete, </Text>
					<Text color="yellow">a</Text>
					<Text> to add, </Text>
					<Text color="yellow">h</Text>
					<Text> for help.</Text>
				</Text>
			</Box>
			<Secrets
				config={config}
				selectedIndex={index}
				loadedSecrets={loadedSecrets}
				loadingSecrets={loadingSecrets}
				deletingName={toDelete}
				sources={sources}
			/>
			{Object.entries(warnings).map(([name, warning]) => (
				<Box key={`warning-${name}`} marginTop={1}>
					<Text color="yellow">⚠️ {warning}</Text>
				</Box>
			))}
			{toDelete && (
				<Box marginTop={1}>
					<Text color="red" bold>
						Delete {toDelete}? (y/n)
					</Text>
				</Box>
			)}
		</Box>
	);
};

export default MainView;
