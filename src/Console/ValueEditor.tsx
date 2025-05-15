import { Box } from "ink";
import type { SecretConfig, SecretSource } from "../types.js";

interface ValueEditorProps {
	secret: SecretConfig;
	name: string;
	source: SecretSource;
	configPath: string;
	onComplete: (value: string) => void;
}

const ValueEditor = ({ secret, name, source, configPath, onComplete }: ValueEditorProps) => {
	return (
		<Box borderStyle="round" borderColor="gray" padding={1}>
			{source.renderEditor({
				secret,
				name,
				configPath,
				onComplete,
			})}
		</Box>
	);
};

export default ValueEditor;
