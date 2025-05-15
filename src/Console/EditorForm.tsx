import { Box, Text, useInput } from "ink";
import type { SecretSource } from "../types.js";

interface EditorFormProps {
	name: string;
	value: string;
	currentField: "name" | "source" | "value" | "secret";
	sourceIndex: number;
	sources: SecretSource[];
	fetchedValue: string;
	isLoading: boolean;
	onNameChange: (name: string) => void;
	onFieldChange: (field: "name" | "source" | "value" | "secret") => void;
	onSourceChange: (index: number) => void;
	onEditMode: () => void;
	onSave: () => void;
	onCancel: () => void;
}

const EditorForm = ({
	name,
	value,
	currentField,
	sourceIndex,
	sources,
	fetchedValue,
	isLoading,
	onNameChange,
	onFieldChange,
	onSourceChange,
	onEditMode,
	onSave,
	onCancel,
}: EditorFormProps) => {
	useInput((input, key) => {
		if (key.escape) {
			onCancel();
		} else if (key.leftArrow || key.return) {
			onSave();
		} else if (key.upArrow) {
			if (currentField === "secret") {
				onFieldChange("value");
			} else if (currentField === "value") {
				onFieldChange("source");
			} else if (currentField === "source") {
				onFieldChange("name");
			}
		} else if (key.downArrow) {
			if (currentField === "name") {
				onFieldChange("source");
			} else if (currentField === "source") {
				onFieldChange("value");
			} else if (currentField === "value") {
				onFieldChange("secret");
			}
		} else if (key.tab && currentField === "source") {
			onSourceChange((sourceIndex + 1 + sources.length) % sources.length);
		} else if (currentField === "name") {
			if (key.backspace || key.delete) {
				onNameChange(name.slice(0, -1));
			} else if (input.length === 1) {
				onNameChange(name + input);
			}
		} else if (key.rightArrow && (currentField === "value" || currentField === "secret")) {
			onEditMode();
		}
	});

	return (
		<Box borderStyle="round" borderColor="gray" flexDirection="column" padding={1}>
			<Box marginBottom={1}>
				<Text bold={currentField === "name"}>Name: </Text>
				<Text color={currentField === "name" ? "yellow" : "blue"}>{name}</Text>
			</Box>

			<Box marginBottom={1}>
				<Box flexDirection="row" alignItems="center">
					<Text bold={currentField === "source"}>Source: </Text>
					<Box gap={1} flexDirection="row" alignItems="center" marginLeft={1}>
						{sources.map((source, index) => (
							<Box
								key={source.name}
								borderStyle="round"
								borderColor={currentField === "source" && index === sourceIndex ? "yellow" : "gray"}
								paddingX={1}
								minWidth={8}
								justifyContent="center"
							>
								<Text color={index === sourceIndex ? (currentField === "source" ? "yellow" : "blue") : undefined}>
									{source.name}
								</Text>
							</Box>
						))}
					</Box>
					{currentField === "source" && (
						<Box marginLeft={1}>
							<Text>
								(Use <Text color="yellow">tab</Text> to change source)
							</Text>
						</Box>
					)}
				</Box>
			</Box>

			<Box marginBottom={1}>
				<Text bold={currentField === "value"}>Value: </Text>
				<Text color={currentField === "value" ? "yellow" : "blue"}>{value}</Text>
				{currentField === "value" && (
					<Box marginLeft={1}>
						<Text>
							(Press <Text color="yellow">→</Text> to edit {sources[sourceIndex].referenceName})
						</Text>
					</Box>
				)}
			</Box>

			<Box>
				<Text bold={currentField === "secret"}>Secret: </Text>
				{isLoading ? (
					<Text color={currentField === "secret" ? "yellow" : "blue"}>Loading...</Text>
				) : (
					<Text color={currentField === "secret" ? "yellow" : "blue"}>{fetchedValue}</Text>
				)}
				{currentField === "secret" && (
					<Box marginLeft={1}>
						<Text>
							(Press <Text color="yellow">→</Text> to edit {sources[sourceIndex].referenceName})
						</Text>
					</Box>
				)}
			</Box>
		</Box>
	);
};

export default EditorForm;
