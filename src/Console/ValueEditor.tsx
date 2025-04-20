import { Box } from "ink";
import { SecretConfig, SecretSource } from "../types.js";

interface ValueEditorProps {
  secret: SecretConfig;
  name: string;
  source: SecretSource;
  onComplete: (value: string) => void;
}

const ValueEditor = ({
  secret,
  name,
  source,
  onComplete,
}: ValueEditorProps) => {
  return (
    <Box borderStyle="round" borderColor="gray" padding={1}>
      {source.renderEditor({
        secret,
        name,
        onComplete,
      })}
    </Box>
  );
};

export default ValueEditor;
