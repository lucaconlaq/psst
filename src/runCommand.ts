import { spawn } from "child_process";

export const runCommand = (
  cmd: string,
  args: string[],
  env: NodeJS.ProcessEnv
) => {
  const proc = spawn(cmd, args, {
    stdio: "inherit",
    env,
  });

  proc.on("error", (error) => {
    console.error(`Error: ${error.message}`);
    process.exit(1);
  });

  proc.on("exit", (code) => {
    process.exit(code ?? 0);
  });
};
