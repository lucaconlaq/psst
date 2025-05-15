import { exec } from "node:child_process";
import { promisify } from "node:util";
import { vaultSource } from "./vault.js";

const execAsync = promisify(exec);

interface VaultData {
	folders: string[];
	items: string[];
}

export const fetchItemsAndFolder = async (path: string): Promise<VaultData> => {
	const result: VaultData = {
		folders: [],
		items: [],
	};

	try {
		const { stdout: listStdout } = await execAsync(`${vaultSource.executable} kv list -format=json ${path}`, {
			encoding: "utf8",
		});
		const listData = JSON.parse(listStdout);
		if (Array.isArray(listData)) {
			result.folders = listData;
		}
	} catch {
		// Ignore error, folders will be empty array
	}

	try {
		const { stdout: getStdout } = await execAsync(`${vaultSource.executable} kv get -format=json ${path}`, {
			encoding: "utf8",
		});
		const getData = JSON.parse(getStdout);
		if (getData?.data?.data) {
			result.items = Object.keys(getData.data.data);
		}
	} catch {
		// Ignore error, items will be empty array
	}

	return result;
};

export const fetchKVMounts = async (executable: string): Promise<string[] | null> => {
	try {
		const { stdout } = await execAsync(`${executable} read -format=json sys/internal/ui/mounts`, { encoding: "utf8" });
		const mountsData = JSON.parse(stdout).data.secret as Record<string, { type: string }>;
		return Object.entries(mountsData)
			.filter(([_, mount]) => mount.type === "kv")
			.map(([path]) => path);
	} catch (err) {
		return null;
	}
};

export const fetchSecretFromPath = async (pathWithSecret: string): Promise<string> => {
	const [path, secretKey] = pathWithSecret.split("#");
	if (!path || !secretKey) {
		throw new Error("Invalid path format. Expected format: /path/path/path#secret");
	}

	try {
		const { stdout } = await execAsync(`${vaultSource.executable} kv get -format=json ${path}`, { encoding: "utf8" });
		const data = JSON.parse(stdout);
		const secret = data?.data?.data?.[secretKey];

		if (secret === undefined) {
			throw new Error(`Secret key '${secretKey}' not found in path '${path}'`);
		}

		return secret;
	} catch (error) {
		if (error instanceof Error) {
			throw new Error(`Failed to fetch secret: ${error.message}`);
		}
		throw new Error("Failed to fetch secret: Unknown error");
	}
};
