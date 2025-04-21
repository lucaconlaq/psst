import { Box, Text } from "ink";

interface VaultPathEditorProps {
  mountPath: string;
  onBack: () => void;
}

export const VaultPathEditor = ({
  mountPath,
  onBack,
}: VaultPathEditorProps) => {
  return (
    <Box flexDirection="column">
      <Box marginBottom={1}>
        <Text bold>Current Path: {mountPath}</Text>
      </Box>
      <Text>Empty</Text>
    </Box>
  );
};
