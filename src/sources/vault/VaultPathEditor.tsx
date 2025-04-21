import { Box, Text, useInput } from "ink";
import { useState, useEffect } from "react";
import { fetchItemsAndFolder } from "./vaultFetchData.js";

interface VaultPathEditorProps {
  mountPath: string;
  onBack: () => void;
  onNavigate: (path: string) => void;
  onComplete: (value: string) => void;
}

interface VaultData {
  folders: string[];
  items: string[];
}

const MAX_VISIBLE_ITEMS = 5;

const joinPaths = (base: string, segment: string): string => {
  // Remove trailing slashes from base and leading slashes from segment
  const cleanBase = base.replace(/\/+$/, "");
  const cleanSegment = segment.replace(/^\/+/, "");
  return `${cleanBase}/${cleanSegment}`;
};

const getParentPath = (path: string): string | null => {
  const parts = path.split("/").filter(Boolean);
  if (parts.length <= 1) return null;
  return parts.slice(0, -1).join("/");
};

export const VaultPathEditor = ({
  mountPath,
  onBack,
  onNavigate,
  onComplete,
}: VaultPathEditorProps) => {
  const [isLoading, setIsLoading] = useState(true);
  const [folders, setFolders] = useState<string[]>([]);
  const [items, setItems] = useState<string[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);

  // Reset search and selection when path changes
  useEffect(() => {
    setSearchQuery("");
    setSelectedIndex(0);
  }, [mountPath]);

  useEffect(() => {
    const loadData = async () => {
      setIsLoading(true);
      const result = await fetchItemsAndFolder(mountPath);
      setFolders(result.folders);
      setItems(result.items);
      setIsLoading(false);
    };

    loadData();
  }, [mountPath]);

  const filteredFolders = folders.filter((folder) =>
    folder.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const filteredItems = items.filter((item) =>
    item.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const totalItems = filteredFolders.length + filteredItems.length;

  useInput((input, key) => {
    if (key.leftArrow) {
      const parentPath = getParentPath(mountPath);
      if (parentPath === null) {
        // We're at the root level, go back to mounts
        onBack();
      } else {
        // Go back one level
        onNavigate(parentPath);
      }
    } else if (key.rightArrow) {
      const selectedItem =
        selectedIndex < filteredFolders.length
          ? filteredFolders[selectedIndex]
          : filteredItems[selectedIndex - filteredFolders.length];

      if (selectedIndex < filteredFolders.length) {
        // It's a folder, navigate to it
        onNavigate(joinPaths(mountPath, selectedItem));
      } else {
        // It's an item, complete with the full path
        onComplete(`/${mountPath}#${selectedItem}`);
      }
    } else if (key.upArrow) {
      setSelectedIndex((prev) => Math.max(0, prev - 1));
    } else if (key.downArrow) {
      setSelectedIndex((prev) => Math.min(totalItems - 1, prev + 1));
    } else if (input.length > 0 && !key.return) {
      setSearchQuery((prev) => prev + input);
    } else if (key.backspace || key.delete) {
      setSearchQuery((prev) => prev.slice(0, -1));
    }
  });

  const renderList = () => {
    const startIndex = Math.max(0, selectedIndex - MAX_VISIBLE_ITEMS + 1);
    const endIndex = Math.min(totalItems, startIndex + MAX_VISIBLE_ITEMS);
    const visibleFolders = filteredFolders.slice(
      Math.max(0, startIndex),
      Math.min(filteredFolders.length, endIndex)
    );
    const visibleItems = filteredItems.slice(
      Math.max(0, startIndex - filteredFolders.length),
      Math.min(filteredItems.length, endIndex - filteredFolders.length)
    );

    return (
      <Box flexDirection="column">
        {visibleFolders.map((folder, index) => (
          <Text
            key={folder}
            color={startIndex + index === selectedIndex ? "yellow" : undefined}
          >
            📂 {folder}
          </Text>
        ))}
        {visibleItems.map((item, index) => (
          <Text
            key={item}
            color={
              startIndex + filteredFolders.length + index === selectedIndex
                ? "yellow"
                : undefined
            }
          >
            🔑 {item}{" "}
            <Text color="gray">
              ({mountPath}#{item})
            </Text>
          </Text>
        ))}
        {totalItems > MAX_VISIBLE_ITEMS && (
          <Text color="gray">
            {startIndex > 0 && "↑ "}
            {endIndex < totalItems && "↓ "}
            Showing {startIndex + 1}-{endIndex} of {totalItems} items
          </Text>
        )}
      </Box>
    );
  };

  return (
    <Box flexDirection="column">
      <Box marginBottom={1}>
        <Text bold>Current Path: {mountPath}</Text>
      </Box>
      <Box marginBottom={1}>
        <Text>
          Search: <Text color="yellow">{searchQuery}</Text>
        </Text>
      </Box>
      {isLoading ? <Text>Loading...</Text> : renderList()}
    </Box>
  );
};
