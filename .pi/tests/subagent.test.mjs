// Run with Node 24+ and Pi's typebox available via NODE_PATH.
// No provider calls: a fake Pi CLI exercises the real process supervisor.
import assert from "node:assert/strict";
import { test } from "node:test";
import { stripTypeScriptTypes, createRequire } from "node:module";
import { readFileSync, writeFileSync, mkdtempSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { execFileSync } from "node:child_process";

const require = createRequire(import.meta.url);
const source = stripTypeScriptTypes(readFileSync(new URL("../extensions/subagent.ts", import.meta.url), "utf8"))
  .replace('from "typebox"', `from ${JSON.stringify(pathToFileURL(require.resolve("typebox")).href)}`);
const { default: extension } = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);

await test("subagent process lifecycle", async (t) => {
  const bin = mkdtempSync(join(tmpdir(), "subagent-test-"));
  const path = process.env.PATH;
  const timeout = execFileSync("which", ["timeout"], { encoding: "utf8" }).trim();
  // Shorten only the deadline test, retaining GNU timeout and the same group.
  writeFileSync(join(bin, "timeout"), `#!/bin/sh
for arg do last="$arg"; done
if [ "$last" = deadline ]; then
  exec '${timeout}' --signal=TERM --kill-after=0.1s 0.3s pi -- deadline
fi
exec '${timeout}' "$@"
`, { mode: 0o700 });
  writeFileSync(join(bin, "pi"), `#!/usr/bin/env node
const task = process.argv.at(-1);
const emit = (e) => process.stdout.write(JSON.stringify(e) + '\\n');
if (process.env.PSST_SUBAGENT !== '1') process.exit(9);
if (task === 'hold' || task === 'deadline') {
  emit({type:'tool_execution_start',toolCallId:'1',toolName:'bash',args:{secret:'must not leak'}});
  setInterval(() => {}, 1000);
} else if (task === 'malformed') { console.log('not json'); }
else if (task === 'truncated') { process.stdout.write('{"type":"agent_settled"}'); }
else {
  const text = JSON.stringify({type:'message_end',message:{role:'assistant',content:[{type:'text',text:'done 🌍\\u2028ok'}],stopReason:'stop',usage:{input:3,output:2,totalTokens:5,cost:{total:0.01}}}}) + '\\n';
  process.stdout.write(text.slice(0,15));
  setTimeout(() => { process.stdout.write(text.slice(15)); emit({type:'agent_settled',aborted:false}); if(task==='failure') process.exitCode=7; }, 100);
}
`, { mode: 0o700 });
  process.env.PATH = `${bin}:${path}`;
  const tools = new Map(), handlers = new Map();
  extension({ registerTool: (tool) => tools.set(tool.name, tool), on: (event, fn) => handlers.set(event, fn) });
  const call = (name, params, signal) => tools.get(name).execute("test", params, signal, undefined, { cwd: bin });
  const start = async (prompt) => (await call("subagent_start", { cwd: bin, prompt, label: prompt })).details;
  const wait = (ids, signal) => call("subagent_wait", { ids }, signal);
  try {
    await t.test("parallel children, LF framing, private logs, usage collected once", async () => {
      const a = await start("success"), b = await start("success");
      const r = await wait([a.id, b.id]);
      assert.equal(r.details.length, 2);
      for (const job of r.details) {
        assert.equal(job.state, "exited");
        assert.equal(job.summary, "done 🌍\u2028ok");
        assert.equal(job.exitCode, 0);
        assert.equal(statSync(join(job.logs, "events.jsonl")).mode & 0o777, 0o600);
        assert.ok(readFileSync(join(job.logs, "exit-status"), "utf8"));
      }
      assert.equal(r.usage.input, 6);
      assert.equal((await wait([a.id, b.id])).usage.input, 0);
    });
    await t.test("failed, malformed, and truncated output block", async () => {
      for (const prompt of ["failure", "malformed", "truncated"]) {
        const job = await start(prompt);
        assert.equal((await wait([job.id])).details[0].state, "blocked");
      }
    });
    await t.test("aborted wait preserves child, metadata excludes arguments, cancel cleans group", async () => {
      const job = await start("hold");
      await new Promise((done) => setTimeout(done, 200));
      const r = await wait([job.id], AbortSignal.abort());
      assert.equal(r.details[0].state, "running");
      assert.equal(r.details[0].active[0].tool, "bash");
      assert.ok(!r.content[0].text.includes("must not leak"));
      const cancelled = await call("subagent_cancel", { ids: [job.id] });
      assert.equal(cancelled.details[0].state, "blocked");
      assert.throws(() => process.kill(-job.pid, 0), { code: "ESRCH" });
      await call("subagent_cancel", { ids: [job.id] });
    });
    await t.test("OS deadline blocks and terminates the child", async () => {
      const job = await start("deadline");
      const r = (await wait([job.id])).details[0];
      assert.equal(r.state, "blocked");
      assert.equal(r.exitCode, 124);
      assert.throws(() => process.kill(-job.pid, 0), { code: "ESRCH" });
    });
    await t.test("unknown handles rejected; shutdown cleans running children", async () => {
      await assert.rejects(wait(["unknown"]), /Unknown subagent/);
      const job = await start("hold");
      await handlers.get("session_shutdown")();
      assert.throws(() => process.kill(-job.pid, 0), { code: "ESRCH" });
    });
    await t.test("children cannot register delegation tools", () => {
      process.env.PSST_SUBAGENT = "1";
      try { extension({ registerTool: () => assert.fail("recursive delegation") }); }
      finally { delete process.env.PSST_SUBAGENT; }
    });
  } finally {
    await handlers.get("session_shutdown")();
    process.env.PATH = path;
    rmSync(bin, { recursive: true, force: true });
  }
});
