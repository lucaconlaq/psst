import { ReactNode } from "react";

export interface SecretConfig {
  value: string;
  source: "manual" | "op" | "vault" | "shell";
}

export interface SecretsConfig {
  [key: string]: SecretConfig;
}

export type Env = Record<string, string>;

export enum SecretFetchResultType {
  Success = "success",
  Warning = "warning",
}

export type SecretFetchResult =
  | { type: SecretFetchResultType.Success; value: string }
  | { type: SecretFetchResultType.Warning; warning: string };

export interface SecretSource {
  name: string;
  referenceName: string;
  safeValue: (secret: SecretConfig) => string;
  fetchSecret: (
    secret: SecretConfig,
    key: string
  ) => Promise<SecretFetchResult>;
  editorOptions: SecretEditorOptions[];
  renderEditor: (props: {
    secret: SecretConfig;
    name: string;
    onComplete: (value: string) => void;
  }) => ReactNode;
}

export interface SecretEditorOptions {
  key: string;
  description: string;
}
