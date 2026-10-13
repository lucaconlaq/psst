import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, openSync, closeSync, writeSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

// Session-local handles only. Logs survive cleanup; jobs do not survive reload.
const pause = (ms: number) => new Promise<void>((done) => setTimeout(done, ms));
const idsSchema = Type.Object({ ids: Type.Array(Type.String(), { minItems: 1 }) });

export default function (pi: ExtensionAPI) {
	// Children retain normal project/provider extensions, but cannot delegate recursively.
	if (process.env.PSST_SUBAGENT === "1") return;
	const jobs = new Map<string, Awaited<ReturnType<typeof launch>>>();

	function groupExists(pgid: number) {
		try { process.kill(-pgid, 0); return true; }
		catch (error) {
			if ((error as NodeJS.ErrnoException).code === "ESRCH") return false;
			throw error;
		}
	}

	function send(pgid: number, signal: NodeJS.Signals) {
		try { process.kill(-pgid, signal); }
		catch (error) { if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error; }
	}

	async function terminate(pgid: number) {
		if (!groupExists(pgid)) return;
		send(pgid, "SIGTERM");
		const end = Date.now() + 10_000;
		while (groupExists(pgid) && Date.now() < end) await pause(100);
		if (groupExists(pgid)) {
			send(pgid, "SIGKILL");
			for (let i = 0; i < 20 && groupExists(pgid); i++) await pause(100);
		}
		if (groupExists(pgid)) throw new Error(`Process group ${pgid} still exists; cleanup unconfirmed`);
	}

	async function launch(cwd: string, prompt: string, label: string) {
		if (process.platform !== "linux") throw new Error("Subagent supervision requires Linux, GNU timeout, and ps");
		execFileSync("timeout", ["--version"], { timeout: 5000 });
		const dir = mkdtempSync(join(tmpdir(), "psst-subagent-"));
		const started = Date.now();
		const deadline = started + 3_600_000;
		const stdout = openSync(join(dir, "events.jsonl"), "wx", 0o600);
		const stderr = openSync(join(dir, "stderr.log"), "wx", 0o600);
		const active = new Map<string, { tool: string; started: number }>();
		let lastEvent = started, event = "starting", lastCompleted = "", failures = 0;
		let retries = 0, compactions = 0, summary = "", stopReason = "", summaryTruncated = false;
		let finishedAt = 0;
		let problem = "", settled = false, finished = false, collected = false;
		const usage = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0,
			cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } };
		let exitCode: number | null = null, exitSignal: string | null = null;
		let cancellation: Promise<void> | undefined;
		let buffer = "";
		// detached establishes a dedicated session/group. GNU timeout enforces the
		// deadline even if the parent event loop is busy or the parent crashes.
		const child = spawn("timeout", ["--signal=TERM", "--kill-after=10s", "3600s",
			"pi", "--approve", "--mode", "json", "--no-session", "--", prompt], {
			cwd, detached: true, stdio: ["ignore", "pipe", "pipe"],
			env: { ...process.env, PSST_SUBAGENT: "1" },
		});
		child.stdout.setEncoding("utf8");
		child.stdout.on("data", (chunk: string) => {
			writeSync(stdout, chunk);
			buffer += chunk;
			let newline: number;
			while ((newline = buffer.indexOf("\n")) !== -1) {
				const line = buffer.slice(0, newline);
				buffer = buffer.slice(newline + 1);
				try {
					const e = JSON.parse(line);
					if (typeof e.type !== "string") throw new Error("Missing event type");
					lastEvent = Date.now(); event = e.type;
					if (e.type === "tool_execution_start") active.set(e.toolCallId, { tool: e.toolName, started: lastEvent });
					if (e.type === "tool_execution_end") {
						active.delete(e.toolCallId); lastCompleted = e.toolName;
						if (e.isError) failures++;
					}
					if (e.type === "auto_retry_start" || e.type === "summarization_retry_scheduled") retries++;
					if (e.type === "compaction_start") compactions++;
					if (e.type === "agent_start") settled = false;
					if (e.type === "agent_settled") { settled = true; if (e.aborted) problem = "Child aborted"; }
					if (e.type === "message_end" && e.message?.role === "assistant") {
						const text = (e.message.content ?? []).filter((p: any) => p.type === "text").map((p: any) => p.text).join("\n");
						summary = text.slice(0, 16_000); summaryTruncated = text.length > 16_000;
						stopReason = e.message.stopReason ?? "";
						for (const key of ["input", "output", "cacheRead", "cacheWrite", "totalTokens"] as const) usage[key] += e.message.usage?.[key] ?? 0;
						for (const key of ["input", "output", "cacheRead", "cacheWrite", "total"] as const) usage.cost[key] += e.message.usage?.cost?.[key] ?? 0;
					}
				} catch { problem = "Malformed JSON event; inspect events.jsonl"; }
			}
		});
		child.stderr.on("data", (chunk: Buffer) => writeSync(stderr, chunk));
		child.on("error", (error) => { problem = error.message; });
		// Descendants may retain the output pipes after the leader exits. Clean
		// them on exit, not just close (which waits for those pipes).
		let cleanup = Promise.resolve();
		child.on("exit", () => {
			cleanup = terminate(child.pid!).catch((error) => { problem = String(error); });
		});
		const closed = new Promise<void>((done) => {
			child.on("close", async (code, signal) => {
				exitCode = code; exitSignal = signal;
				closeSync(stdout); closeSync(stderr);
				if (buffer) problem = "Truncated JSON event; inspect events.jsonl";
				await cleanup;
				try { writeFileSync(join(dir, "exit-status"), JSON.stringify({ code, signal, problem }), { mode: 0o600 }); }
				catch (error) { problem = `Exit evidence could not be written: ${error}`; }
				finishedAt = Date.now(); finished = true; done();
			});
		});
		try {
			await new Promise<void>((done, reject) => {
				child.once("spawn", done); child.once("error", reject);
			});
			const pgid = Number(execFileSync("ps", ["-o", "pgid=", "-p", String(child.pid)], { encoding: "utf8", timeout: 5000 }).trim());
			if (pgid !== child.pid) throw new Error("Isolated child process group could not be verified");
		} catch (error) {
			if (child.pid) await terminate(child.pid);
			await closed;
			throw error;
		}
		try {
			writeFileSync(join(dir, "job.json"), JSON.stringify({ label, cwd, started, deadline, pid: child.pid, pgid: child.pid }), { mode: 0o600 });
		} catch (error) { await terminate(child.pid!); await closed; throw error; }
		return {
			id: dir, closed,
			collectUsage() {
				if (!finished || collected) return undefined;
				collected = true;
				return usage;
			},
			cancel() {
				return cancellation ??= (async () => {
					if (!finished) { problem = "Cancelled"; await terminate(child.pid!); }
					await closed;
				})();
			},
			snapshot() {
				const now = Date.now();
				return {
					id: dir, label, pid: child.pid, logs: dir, deadline: new Date(deadline).toISOString(),
					state: !finished ? "running" : problem || exitCode !== 0 || !settled || !summary || ["error", "aborted"].includes(stopReason) ? "blocked" : "exited",
					elapsedSeconds: Math.round(((finishedAt || now) - started) / 1000),
					silenceSeconds: Math.round((now - lastEvent) / 1000), event,
					active: [...active.values()].map((a) => ({ tool: a.tool, seconds: Math.round((now - a.started) / 1000) })),
					lastCompleted, failures, retries, compactions,
					...(finished ? { exitCode, exitSignal, problem, settled, stopReason,
						summary, summaryTruncated } : {}),
				};
			},
		};
	}

	function select(ids: string[]) {
		return [...new Set(ids)].map((id) => {
			const job = jobs.get(id);
			if (!job) throw new Error(`Unknown subagent ${id}; handles do not survive reload`);
			return job;
		});
	}
	function result(data: unknown) {
		return { content: [{ type: "text" as const, text: JSON.stringify(data) }], details: data };
	}
	function collect(selected: Awaited<ReturnType<typeof launch>>[]) {
		const usage = { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0,
			cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } };
		for (const job of selected) {
			const u = job.collectUsage();
			if (!u) continue;
			for (const key of ["input", "output", "cacheRead", "cacheWrite", "totalTokens"] as const) usage[key] += u[key];
			for (const key of ["input", "output", "cacheRead", "cacheWrite", "total"] as const) usage.cost[key] += u.cost[key];
		}
		return { ...result(selected.map((j) => j.snapshot())), usage };
	}

	pi.registerTool({
		name: "subagent_start", label: "Start subagent",
		description: "Launch a fresh Pi child in a trusted working directory with a 60-minute deadline. Supply a short self-contained prompt. Collect with subagent_wait; cancel before abandoning work.",
		parameters: Type.Object({ cwd: Type.String(), prompt: Type.String(), label: Type.String() }),
		async execute(_id, params, signal, _update, ctx) {
			if (signal?.aborted) throw new Error("Launch cancelled");
			const job = await launch(resolve(ctx.cwd, params.cwd), params.prompt, params.label);
			jobs.set(job.id, job);
			if (signal?.aborted) { await job.cancel(); throw new Error("Launch cancelled"); }
			return result(job.snapshot());
		},
	});
	pi.registerTool({
		name: "subagent_wait", label: "Wait for subagents",
		description: "Wait up to 40 seconds for all supplied children, then return compact activity snapshots or final summaries. Repeat while running; no sleep or log parsing needed. Aborting a wait does not cancel children. An exited process is not proof of task completion.",
		parameters: idsSchema,
		async execute(_id, params, signal) {
			const selected = select(params.ids);
			const end = Date.now() + 40_000;
			while (selected.some((j) => j.snapshot().state === "running") && Date.now() < end && !signal?.aborted) await pause(100);
			return collect(selected);
		},
	});
	pi.registerTool({
		name: "subagent_cancel", label: "Cancel subagents",
		description: "Terminate only owned child process groups with TERM, then KILL after 10 seconds. Confirm cleanup before further work. Retains logs and worktree changes.",
		parameters: idsSchema,
		async execute(_id, params) {
			const selected = select(params.ids);
			await Promise.all(selected.map((j) => j.cancel()));
			return collect(selected);
		},
	});
	pi.on("session_shutdown", async () => {
		await Promise.all([...jobs.values()].map((j) => j.cancel()));
	});
}
