import { Box, Text } from "ink";
import { useState } from "react";
import { SecretsConfig, SecretConfig } from "../types.js";
import Editor from "./Editor.js";
import Help from "./Help.js";
import MainView from "./MainView.js";
import { SecretSource } from "../types.js";
import { writeFileSync, existsSync, mkdirSync } from "fs";
import { dirname } from "path";

interface ConsoleProps {
  config: SecretsConfig;
  configFile: string;
  sources: SecretSource[];
}

const Console = ({ config, configFile, sources }: ConsoleProps) => {
  type View = "main" | "help" | "editing";
  const [currentView, setCurrentView] = useState<View>("main");
  const [editingKey, setEditingKey] = useState<string | null>(null);
  const [currentConfig, setCurrentConfig] = useState<SecretsConfig>(config);
  const configPath = dirname(configFile);

  const saveConfig = (config: SecretsConfig): void => {
    const configDir = dirname(configFile);
    if (!existsSync(configDir)) {
      mkdirSync(configDir, { recursive: true });
    }
    writeFileSync(configFile, JSON.stringify(config, null, 2));
  };

  const handleSaveSecret = (
    previousName: string | null,
    name: string,
    secret: SecretConfig
  ): void => {
    const newConfig = { ...currentConfig };

    // If we're editing and the name changed, remove the old name
    if (previousName && previousName !== name) {
      delete newConfig[previousName];
    }

    // Add/update the secret
    newConfig[name] = secret;
    setCurrentConfig(newConfig);
    saveConfig(newConfig);
    setCurrentView("main");
  };

  const newSecretName = (): string => {
    // Find the highest SECRET_X number, or start from 1 if none exist
    const names = Object.keys(currentConfig);
    const maxIndex = names.reduce((max, name) => {
      const match = name.match(/^SECRET_(\d+)$/);
      if (match) {
        const index = parseInt(match[1]);
        return Math.max(max, index);
      }
      return max;
    }, 0);
    return `SECRET_${maxIndex + 1}`;
  };

  return (
    <Box flexDirection="column">
      <Box marginBottom={1}>
        <Text>
          🤫 psst <Text color="blue">[</Text>
          <Text color="blue" underline>
            {configFile}
          </Text>
          <Text color="blue">]</Text>
        </Text>
      </Box>

      {currentView === "help" && <Help onBack={() => setCurrentView("main")} />}

      {currentView === "editing" && (
        <Editor
          previousName={editingKey}
          initialName={editingKey || newSecretName()}
          configPath={configPath}
          secret={
            editingKey
              ? currentConfig[editingKey]
              : { source: "manual" as const, value: "" }
          }
          sources={sources}
          onCancel={() => setCurrentView("main")}
          onSave={handleSaveSecret}
        />
      )}

      {currentView === "main" && (
        <MainView
          config={currentConfig}
          configPath={configPath}
          sources={sources}
          onEdit={(key) => {
            setEditingKey(key);
            setCurrentView("editing");
          }}
          onDelete={(key) => {
            const newConfig = { ...currentConfig };
            delete newConfig[key];
            setCurrentConfig(newConfig);
            saveConfig(newConfig);
          }}
          onHelp={() => setCurrentView("help")}
        />
      )}
    </Box>
  );
};

export default Console;
