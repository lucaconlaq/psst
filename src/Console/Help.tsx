import { Box, Text, useInput } from "ink";

interface HelpProps {
	onBack: () => void;
}

const Help = ({ onBack }: HelpProps) => {
	useInput((input) => {
		if (input === "h") {
			onBack();
		}
	});

	return (
		<Box flexDirection="column">
			<Box marginBottom={2} flexDirection="column">
				<Text bold>Usage:</Text>
				<Text>
					<Text color="yellow">psst</Text> - Opens psst console
				</Text>
				<Text>
					<Text color="yellow">psst {"<any command>"}</Text> - Runs any command with your secrets injected into its
					environment
				</Text>
			</Box>
			<Box marginBottom={2} flexDirection="column">
				<Text bold>Examples:</Text>
				<Text>
					<Text color="blue">psst zsh</Text>
				</Text>
				<Text>
					<Text color="blue">psst npm run dev</Text>
				</Text>
				<Text>
					<Text color="blue">psst env</Text>
				</Text>
				<Text>
					<Text color="blue">psst sh -c 'echo $A_SECRET'</Text>
				</Text>
			</Box>
			<Box marginBottom={2} flexDirection="column">
				<Text bold>Console Commands:</Text>
				<Text>
					<Text color="yellow">q</Text> - Quit the console
				</Text>
				<Text>
					<Text color="yellow">p</Text> - Toggle password visibility for selected secret
				</Text>
				<Text>
					<Text color="yellow">↑↓</Text> - Navigate between secrets
				</Text>
				<Text>
					<Text color="yellow">→</Text> - Edit the selected secret
				</Text>
				<Text>
					<Text color="yellow">d</Text> - Delete the selected secret
				</Text>
				<Text>
					<Text color="yellow">a</Text> - Add a new secret
				</Text>
				<Text>
					<Text color="yellow">h</Text> - Show/hide help menu
				</Text>
			</Box>
			<Box marginTop={1}>
				<Text>
					Press <Text color="yellow">h</Text> to return to the main view
				</Text>
			</Box>
		</Box>
	);
};

export default Help;
