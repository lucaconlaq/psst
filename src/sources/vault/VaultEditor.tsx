import { Box, Text, useInput } from "ink";
import { useState, useEffect } from "react";
import { vaultSource } from "./vault.js";
import { VaultPathEditor } from "./VaultPathEditor.js";
import { fetchKVMounts } from "./vaultFetchData.js";

interface VaultEditorProps {
	onComplete: (value: string) => void;
}

const MAX_VISIBLE_ITEMS = 5;

export const VaultEditor = ({ onComplete }: VaultEditorProps) => {
	const [mounts, setMounts] = useState<string[]>([]);
	const [selectedIndex, setSelectedIndex] = useState(0);
	const [error, setError] = useState<string | null>(null);
	const [searchQuery, setSearchQuery] = useState("");
	const [filteredMounts, setFilteredMounts] = useState<string[]>([]);
	const [isLoading, setIsLoading] = useState(true);
	const [currentPath, setCurrentPath] = useState<string | null>(null);
	const executable = vaultSource.executable;

	useEffect(() => {
		const loadMounts = async () => {
			setIsLoading(true);
			const kvMounts = await fetchKVMounts(executable);
			if (!kvMounts) {
				setError(
					"Failed to fetch KV mounts. Please ensure you're authenticated with Vault by running 'vault token lookup'.",
				);
				setMounts([]);
				setFilteredMounts([]);
			} else {
				setMounts(kvMounts);
				setFilteredMounts(kvMounts);
				setError(null);
			}
			setIsLoading(false);
		};

		loadMounts();
	}, [executable]);

	useEffect(() => {
		const filtered = mounts.filter((mount) => mount.toLowerCase().includes(searchQuery.toLowerCase()));
		setFilteredMounts(filtered);
		setSelectedIndex(0);
	}, [searchQuery, mounts]);

	useInput((input, key) => {
		if (key.leftArrow) {
			if (currentPath) {
				setCurrentPath(null);
			} else {
				onComplete("");
			}
		} else if (key.rightArrow && filteredMounts[selectedIndex]) {
			setCurrentPath(filteredMounts[selectedIndex]);
		} else if (key.upArrow) {
			setSelectedIndex((prev) => Math.max(0, prev - 1));
		} else if (key.downArrow) {
			setSelectedIndex((prev) => Math.min(filteredMounts.length - 1, prev + 1));
		} else if (input.length > 0 && !key.return) {
			setSearchQuery((prev) => prev + input);
		} else if (key.backspace || key.delete) {
			setSearchQuery((prev) => prev.slice(0, -1));
		}
	});

	const renderList = (items: string[], selectedIndex: number) => {
		const startIndex = Math.max(0, selectedIndex - MAX_VISIBLE_ITEMS + 1);
		const endIndex = Math.min(items.length, startIndex + MAX_VISIBLE_ITEMS);
		const visibleRange = items.slice(startIndex, endIndex);

		return (
			<Box flexDirection="column">
				{visibleRange.map((mount, index) => (
					<Text key={mount} color={startIndex + index === selectedIndex ? "yellow" : undefined}>
						💾 {mount}
					</Text>
				))}
				{items.length > MAX_VISIBLE_ITEMS && (
					<Text color="gray">
						{startIndex > 0 && "↑ "}
						{endIndex < items.length && "↓ "}
						Showing {startIndex + 1}-{endIndex} of {items.length} mounts
					</Text>
				)}
			</Box>
		);
	};

	if (currentPath) {
		return (
			<VaultPathEditor
				mountPath={currentPath}
				onBack={() => setCurrentPath(null)}
				onNavigate={(path) => setCurrentPath(path)}
				onComplete={onComplete}
			/>
		);
	}

	return (
		<Box flexDirection="column">
			{error && (
				<Box marginBottom={1}>
					<Text color="red">⚠️ {error}</Text>
				</Box>
			)}

			<Box marginBottom={1}>
				<Text bold>Available Mounts:</Text>
			</Box>
			<Box marginBottom={1}>
				<Text>
					Search: <Text color="yellow">{searchQuery}</Text>
				</Text>
			</Box>
			{isLoading ? <Text>Loading mounts...</Text> : renderList(filteredMounts, selectedIndex)}
		</Box>
	);
};
