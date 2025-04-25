import { Text } from "ink";
import { SecretConfig } from "../../types.js";

interface TerraformEditorProps {
  secret: SecretConfig;
  name: string;
  onComplete: (value: string) => void;
}

export const TerraformEditor = ({
  secret,
  name,
  onComplete,
}: TerraformEditorProps) => {
  return <Text color="red">Not implemented yet. Coming soon.</Text>;
};
