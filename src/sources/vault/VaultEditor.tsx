import { Box, Text, useInput } from "ink";
import { useState, useEffect } from "react";
import { execSync } from "child_process";

interface VaultEditorProps {
  onComplete: (value: string) => void;
}

export const VaultEditor = ({ onComplete }: VaultEditorProps) => {
  return (
    <Box>
      <Text>Vault Editor</Text>
    </Box>
  );
};
