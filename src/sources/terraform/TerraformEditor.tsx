import { Text, useInput } from "ink";
import { useState } from "react";
import type { SecretConfig } from "../../types.js";

interface TerraformEditorProps {
	secret: SecretConfig;
	name: string;
	configPath: string;
	onComplete: (value: string) => void;
}

export const TerraformEditor = ({ secret, name, configPath, onComplete }: TerraformEditorProps) => {
	const [selectedIndex, setSelectedIndex] = useState(0);
	const [outputs] = useState<string[]>([]); // Empty array for now

	useInput((input, key) => {
		if (key.rightArrow) {
			// If there are no elements available, return early
			if (!secret.value) {
				onComplete("");
				return;
			}
		}
	});

	return <Text color="red">Not implemented yet. Coming soon.</Text>;
};
