import { Box, Text, useInput } from "ink";
import { useState } from "react";
import type { SecretConfig } from "../../types.js";
import { shellSource } from "./shell.js";

interface ShellEditorProps {
	secret: SecretConfig;
	name: string;
	configPath: string;
	onComplete: (value: string) => void;
}

export function ShellEditor({ secret, name, configPath, onComplete }: ShellEditorProps) {
	const [currentValue, setCurrentValue] = useState(secret.value);
	const [output, setOutput] = useState("");
	const [warning, setWarning] = useState("");
	useInput(async (input, key) => {
		if (key.leftArrow) {
			onComplete(currentValue);
		} else if (key.backspace || key.delete) {
			setCurrentValue((prev) => prev.slice(0, -1));
		} else if (input.length > 0 && !key.return) {
			setCurrentValue((prev) => prev + input);
		} else if (key.downArrow) {
			const result = await shellSource.fetchSecret({ value: currentValue, source: secret.source }, name, configPath);
			if (result.type === "success") {
				setOutput(result.value);
				setWarning("");
			} else {
				setOutput("");
				setWarning(result.warning);
			}
		}
	});

	return (
		<Box flexDirection="column">
			<Box marginBottom={1}>
				<Text bold>Shell command: </Text>
				<Text color="yellow">{currentValue}</Text>
			</Box>
			<Box marginBottom={1}>
				<Text bold>Output: </Text>
				<Text color="yellow">{output}</Text>
			</Box>
			{warning && (
				<Box marginBottom={1}>
					<Text color="red">⚠️ {warning}</Text>
				</Box>
			)}
		</Box>
	);
}
