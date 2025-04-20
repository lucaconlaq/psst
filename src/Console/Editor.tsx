import { Box, Text } from "ink";
import { useState, useEffect } from "react";
import { SecretConfig, SecretSource, SecretFetchResultType } from "../types.js";
import EditorForm from "./EditorForm.js";
import ValueEditor from "./ValueEditor.js";

interface EditorProps {
  previousName: string | null;
  initialName: string;
  secret: SecretConfig;
  sources: SecretSource[];
  onCancel: () => void;
  onSave: (
    previousName: string | null,
    name: string,
    secret: SecretConfig
  ) => void;
}

const Editor = ({
  previousName,
  initialName,
  secret,
  sources,
  onCancel,
  onSave,
}: EditorProps) => {
  const [name, setName] = useState(initialName);
  const [currentField, setCurrentField] = useState<
    "name" | "source" | "value" | "secret"
  >("name");
  const [sourceIndex, setSourceIndex] = useState(() => {
    const index = sources.findIndex((s) => s.name === secret.source);
    return index === -1 ? 0 : index;
  });
  const [fetchedValue, setFetchedValue] = useState<string>("");
  const [isLoading, setIsLoading] = useState(true);
  const [showSourceEditor, setShowSourceEditor] = useState(false);
  const [value, setValue] = useState<string>(secret.value);

  const currentSource = sources[sourceIndex];

  useEffect(() => {
    const fetchSecret = async () => {
      setIsLoading(true);
      await new Promise((resolve) => setTimeout(resolve, 50));
      try {
        const result = await currentSource.fetchSecret(
          {
            value,
            source: sources[sourceIndex].name as SecretConfig["source"],
          },
          name
        );
        if (result.type === SecretFetchResultType.Success) {
          setFetchedValue(result.value);
        } else {
          setFetchedValue("");
        }
      } catch (error) {
        setFetchedValue("");
      }
      setIsLoading(false);
    };

    fetchSecret();
  }, [currentSource, secret, value]);

  const handleSave = () => {
    onSave(previousName, name, {
      value,
      source: sources[sourceIndex].name as SecretConfig["source"],
    });
  };

  const handleSourceEditorComplete = (value: string) => {
    setValue(value);
    setShowSourceEditor(false);
  };

  return (
    <Box flexDirection="column">
      <Box marginBottom={1}>
        {showSourceEditor ? (
          <Text>
            Press{" "}
            {currentSource.editorOptions.map((option, index) => (
              <Text key={option.key}>
                <Text color="yellow">{option.key}</Text> to{" "}
                <Text>{option.description}</Text>
                {index < currentSource.editorOptions.length - 1 ? ", " : ""}
              </Text>
            ))}
            .
          </Text>
        ) : (
          <Text>
            Press <Text color="yellow">←</Text> to save and go back,{" "}
            <Text color="yellow">↑↓</Text> to switch fields,{" "}
            <Text color="yellow">Esc</Text> to revert edits and go back.{" "}
          </Text>
        )}
      </Box>
      {showSourceEditor ? (
        <ValueEditor
          secret={secret}
          name={name}
          source={currentSource}
          onComplete={handleSourceEditorComplete}
        />
      ) : (
        <EditorForm
          name={name}
          value={value}
          currentField={currentField}
          sourceIndex={sourceIndex}
          sources={sources}
          fetchedValue={fetchedValue}
          isLoading={isLoading}
          onNameChange={setName}
          onFieldChange={setCurrentField}
          onSourceChange={setSourceIndex}
          onEditMode={() => setShowSourceEditor(true)}
          onSave={handleSave}
          onCancel={onCancel}
        />
      )}
    </Box>
  );
};

export default Editor;
