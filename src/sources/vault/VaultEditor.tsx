import { Box, Text, useInput } from "ink";
import { useState, useEffect } from "react";
import { execSync } from "child_process";
import { vaultSource } from "./vault.js";
import { VaultPathEditor } from "./VaultPathEditor.js";

interface VaultEditorProps {
  onComplete: (value: string) => void;
}

interface Mount {
  path: string;
  type: string;
  description: string;
}

const MAX_VISIBLE_ITEMS = 5;

export const VaultEditor = ({ onComplete }: VaultEditorProps) => {
  const [mounts, setMounts] = useState<Mount[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [filteredMounts, setFilteredMounts] = useState<Mount[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [selectedMount, setSelectedMount] = useState<string | null>(null);
  const executable = vaultSource.executable;

  useEffect(() => {
    try {
      setIsLoading(true);
      const mountsJson = execSync(
        `${executable} read -format=json sys/internal/ui/mounts`,
        {
          encoding: "utf8",
        }
      );
      const mountsData = JSON.parse(mountsJson).data.secret as Record<
        string,
        Mount
      >;
      const kvMounts = Object.entries(mountsData)
        .filter(([_, mount]) => mount.type === "kv")
        .map(([path, mount]) => ({ ...mount, path }));
      setMounts(kvMounts);
      setFilteredMounts(kvMounts);
    } catch (err) {
      setError(
        "Failed to fetch KV mounts. Please ensure you're authenticated with Vault by running 'vault token lookup'."
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    const filtered = mounts.filter((mount) =>
      mount.path.toLowerCase().includes(searchQuery.toLowerCase())
    );
    setFilteredMounts(filtered);
    setSelectedIndex(0);
  }, [searchQuery, mounts]);

  useInput((input, key) => {
    if (key.leftArrow) {
      if (selectedMount) {
        setSelectedMount(null);
      } else {
        onComplete("");
      }
    } else if (key.rightArrow && filteredMounts[selectedIndex]) {
      setSelectedMount(filteredMounts[selectedIndex].path);
    } else if (key.upArrow) {
      setSelectedIndex((prev) => Math.max(0, prev - 1));
    } else if (key.downArrow) {
      setSelectedIndex((prev) => Math.min(filteredMounts.length - 1, prev + 1));
    } else if (input.length > 0 && !key.return) {
      setSearchQuery((prev) => prev + input);
    } else if (key.backspace || key.delete) {
      setSearchQuery((prev) => prev.slice(0, -1));
    }
  });

  const renderList = (items: Mount[], selectedIndex: number) => {
    const startIndex = Math.max(0, selectedIndex - MAX_VISIBLE_ITEMS + 1);
    const endIndex = Math.min(items.length, startIndex + MAX_VISIBLE_ITEMS);
    const visibleRange = items.slice(startIndex, endIndex);

    return (
      <Box flexDirection="column">
        {visibleRange.map((mount, index) => (
          <Text
            key={mount.path}
            color={startIndex + index === selectedIndex ? "yellow" : undefined}
          >
            💾 {mount.path}
          </Text>
        ))}
        {items.length > MAX_VISIBLE_ITEMS && (
          <Text color="gray">
            {startIndex > 0 && "↑ "}
            {endIndex < items.length && "↓ "}
            Showing {startIndex + 1}-{endIndex} of {items.length} mounts
          </Text>
        )}
      </Box>
    );
  };

  if (selectedMount) {
    return (
      <VaultPathEditor
        mountPath={selectedMount}
        onBack={() => setSelectedMount(null)}
      />
    );
  }

  return (
    <Box flexDirection="column">
      {error && (
        <Box marginBottom={1}>
          <Text color="red">⚠️ {error}</Text>
        </Box>
      )}

      <Box marginBottom={1}>
        <Text bold>Available Mounts:</Text>
      </Box>
      <Box marginBottom={1}>
        <Text>
          Search: <Text color="yellow">{searchQuery}</Text>
        </Text>
      </Box>
      {isLoading ? (
        <Text>Loading mounts...</Text>
      ) : (
        renderList(filteredMounts, selectedIndex)
      )}
    </Box>
  );
};
