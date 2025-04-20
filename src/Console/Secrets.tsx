import { Box, Text } from "ink";
import { SecretsConfig, Env, SecretSource } from "../types.js";

interface SecretLineProps {
  name: string;
  source: string;
  secret: string;
  value: string;
  isSelected: boolean;
  isDeleting: boolean;
  isLoading: boolean;
}

const DOTS = "••••••••";

function SecretLine({
  name,
  source,
  secret,
  value,
  isSelected,
  isDeleting,
  isLoading,
}: SecretLineProps) {
  const color = isDeleting ? "red" : isSelected ? "yellow" : undefined;

  return (
    <Box flexDirection="row">
      <Box width={20}>
        <Text color={color}>{name}</Text>
      </Box>
      <Box width={15}>
        <Text color={color}>{source}</Text>
      </Box>
      <Box width={40}>
        <Text color={color}>{value}</Text>
      </Box>
      <Box width={40}>
        <Text color={isLoading ? "yellow" : color}>
          {isLoading ? "loading..." : secret}
        </Text>
      </Box>
    </Box>
  );
}

interface SecretsProps {
  config: SecretsConfig;
  selectedIndex: number;
  loadedSecrets: Env;
  loadingSecrets: Set<string>;
  deletingName: string | null;
  sources: SecretSource[];
}

export const Secrets = ({
  config,
  selectedIndex,
  loadedSecrets,
  loadingSecrets,
  deletingName,
  sources,
}: SecretsProps) => {
  const configEntries = Object.entries(config);

  return (
    <Box flexDirection="column">
      <Box
        flexDirection="column"
        borderStyle="round"
        borderColor="gray"
        padding={1}
      >
        <Box>
          <Box width={20}>
            <Text bold>Name</Text>
          </Box>
          <Box width={15}>
            <Text bold>Source</Text>
          </Box>
          <Box width={40}>
            <Text bold>Value</Text>
          </Box>
          <Box width={40}>
            <Text bold>Secret</Text>
          </Box>
        </Box>

        {configEntries.map(([name, secret], index) => {
          const source = sources.find((s) => s.name === secret.source);
          const isLoading = loadingSecrets.has(name);

          return (
            <Box key={name} flexDirection="column">
              <SecretLine
                name={name}
                source={secret.source}
                secret={loadedSecrets[name] ? loadedSecrets[name] : DOTS}
                value={source ? source.safeValue(secret) : DOTS}
                isSelected={index === selectedIndex}
                isDeleting={name === deletingName}
                isLoading={isLoading}
              />
            </Box>
          );
        })}
      </Box>
    </Box>
  );
};
