import { Box, Text, useInput } from "ink";
import { useState, useEffect } from "react";
import { SecretConfig } from "../../types.js";
import { execSync } from "child_process";
import { opSource } from "./op.js";

interface OpEditorProps {
  secret: SecretConfig;
  name: string;
  onComplete: (value: string) => void;
}

interface Vault {
  id: string;
  name: string;
}

interface Item {
  id: string;
  title: string;
  vault: {
    id: string;
    name: string;
  };
}

interface Field {
  id: string;
  type: string;
  purpose: string;
  label: string;
  value: string;
  reference: string;
}

type State = "vaults" | "items" | "fields";

const MAX_VISIBLE_ITEMS = 5;

export const OpEditor = ({ secret, name, onComplete }: OpEditorProps) => {
  const [state, setState] = useState<State>("vaults");
  const [vaults, setVaults] = useState<Vault[]>([]);
  const [items, setItems] = useState<Item[]>([]);
  const [fields, setFields] = useState<Field[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [currentVault, setCurrentVault] = useState<Vault | null>(null);
  const [currentItem, setCurrentItem] = useState<Item | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [filteredItems, setFilteredItems] = useState<Item[]>([]);
  const [filteredFields, setFilteredFields] = useState<Field[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const executable = opSource.executable;

  useEffect(() => {
    try {
      setIsLoading(true);
      const vaultsJson = execSync(`${executable} vault list --format json`, {
        encoding: "utf8",
      });
      const vaultsData = JSON.parse(vaultsJson) as Vault[];
      setVaults(vaultsData);
    } catch (err) {
      setError(
        "Failed to fetch vaults. Make sure you're logged in to 1Password."
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    if (state === "items") {
      const filtered = items.filter((item) =>
        item.title.toLowerCase().includes(searchQuery.toLowerCase())
      );
      setFilteredItems(filtered);
      setSelectedIndex(0);
    } else if (state === "fields") {
      const filtered = fields.filter((field) => {
        const fieldLabel = field.label || field.purpose || field.type || "";
        return fieldLabel.toLowerCase().includes(searchQuery.toLowerCase());
      });
      setFilteredFields(filtered);
      setSelectedIndex(0);
    }
  }, [searchQuery, items, fields, state]);

  const loadItems = (vault: Vault) => {
    try {
      const itemsJson = execSync(
        `${executable} item list --vault "${vault.id}" --format json`,
        { encoding: "utf8" }
      );
      const itemsData = JSON.parse(itemsJson) as Item[];
      setItems(itemsData);
      setFilteredItems(itemsData);
      setCurrentVault(vault);
      setState("items");
      setSelectedIndex(0);
      setSearchQuery("");
    } catch (err) {
      setError("Failed to fetch items from vault.");
    }
  };

  const loadFields = (item: Item) => {
    try {
      const itemJson = execSync(
        `${executable} item get "${item.id}" --vault "${item.vault.id}" --format json`,
        { encoding: "utf8" }
      );
      const itemData = JSON.parse(itemJson);
      setFields(itemData.fields || []);
      setFilteredFields(itemData.fields || []);
      setCurrentItem(item);
      setState("fields");
      setSelectedIndex(0);
      setSearchQuery("");
    } catch (err) {
      setError("Failed to fetch item fields.");
    }
  };

  useInput((input, key) => {
    if (key.leftArrow) {
      if (state === "fields") {
        setState("items");
        setSelectedIndex(0);
      } else if (state === "items") {
        setState("vaults");
        setSelectedIndex(0);
      } else {
        onComplete(secret.value);
      }
    } else if (key.rightArrow) {
      if (state === "vaults" && vaults[selectedIndex]) {
        loadItems(vaults[selectedIndex]);
      } else if (state === "items" && filteredItems[selectedIndex]) {
        loadFields(filteredItems[selectedIndex]);
      } else if (state === "fields" && filteredFields[selectedIndex]) {
        onComplete(filteredFields[selectedIndex].reference);
      }
    } else if (key.upArrow) {
      setSelectedIndex((prev) => Math.max(0, prev - 1));
    } else if (key.downArrow) {
      const maxIndex =
        state === "vaults"
          ? vaults.length - 1
          : state === "items"
          ? filteredItems.length - 1
          : filteredFields.length - 1;
      setSelectedIndex((prev) => Math.min(maxIndex, prev + 1));
    } else if (input.length > 0 && !key.return) {
      setSearchQuery((prev) => prev + input);
    } else if (key.backspace || key.delete) {
      setSearchQuery((prev) => prev.slice(0, -1));
    }
  });

  const renderList = (items: any[], selectedIndex: number) => {
    const startIndex = Math.max(0, selectedIndex - MAX_VISIBLE_ITEMS + 1);
    const endIndex = Math.min(items.length, startIndex + MAX_VISIBLE_ITEMS);
    const visibleRange = items.slice(startIndex, endIndex);

    return (
      <Box flexDirection="column">
        {visibleRange.map((item, index) => {
          const displayText = item.name || item.title || item.label;
          const key = item.id
            ? `${item.id}-${displayText || index}`
            : `${displayText}-${index}`;

          return (
            <Text
              key={key}
              color={
                startIndex + index === selectedIndex ? "yellow" : undefined
              }
            >
              {state === "fields" ? (
                <>
                  <Text>{displayText}</Text>
                  <Text color="gray"> ({item.reference})</Text>
                </>
              ) : (
                displayText
              )}
            </Text>
          );
        })}
        {items.length > MAX_VISIBLE_ITEMS && (
          <Text color="gray">
            {startIndex > 0 && "↑ "}
            {endIndex < items.length && "↓ "}
            {items.length} items total
          </Text>
        )}
      </Box>
    );
  };

  return (
    <Box flexDirection="column">
      {error && (
        <Box marginBottom={1}>
          <Text color="red">⚠️ {error}</Text>
        </Box>
      )}
      <Box marginBottom={1}>
        <Text>
          Press <Text color="yellow">←</Text> to go back,{" "}
          <Text color="yellow">→</Text> to select,{" "}
          <Text color="yellow">↑↓</Text> to navigate,{" "}
          <Text color="yellow">type</Text> to search
        </Text>
      </Box>
      <Box marginBottom={1}>
        <Text bold>
          {state === "vaults" && "Select a vault:"}
          {state === "items" && `Items in ${currentVault?.name}:`}
          {state === "fields" && `Fields in ${currentItem?.title}:`}
        </Text>
      </Box>
      {state !== "vaults" && (
        <Box marginBottom={1}>
          <Text>
            Search: <Text color="yellow">{searchQuery}</Text>
          </Text>
        </Box>
      )}
      {state === "vaults" && isLoading ? (
        <Text>Loading...</Text>
      ) : (
        <>
          {state === "vaults" && renderList(vaults, selectedIndex)}
          {state === "items" && renderList(filteredItems, selectedIndex)}
          {state === "fields" && renderList(filteredFields, selectedIndex)}
        </>
      )}
    </Box>
  );
};
