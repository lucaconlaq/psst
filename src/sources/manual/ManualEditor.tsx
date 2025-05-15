import { Box, Text, useInput } from "ink";
import { useState } from "react";
import type { SecretConfig } from "../../types.js";

interface ManualEditorProps {
	secret: SecretConfig;
	name: string;
	onComplete: (value: string) => void;
}

export function ManualEditor({ secret, name, onComplete }: ManualEditorProps) {
	const [currentValue, setCurrentValue] = useState(secret.value);

	useInput(async (input, key) => {
		if (key.leftArrow) {
			onComplete(currentValue);
		} else if (key.backspace || key.delete) {
			setCurrentValue((prev) => prev.slice(0, -1));
		} else if (input.length > 0 && !key.return) {
			setCurrentValue((prev) => prev + input);
		}
	});

	return (
		<Box flexDirection="column">
			<Box>
				<Text bold>Value: </Text>
				<Text color="yellow">{currentValue}</Text>
			</Box>
		</Box>
	);
}
