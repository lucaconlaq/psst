# 04: Create encrypted secrets through the masked TUI

**Source:** [Agreed spec](../../../docs/specs/01-local-storage.md), including its linked idea and exclusions.

**What to build:** A user creates an encrypted value and environment mapping in the TUI, then resolves it successfully through the existing runtime consumers. Use tickets 01–03's format, agent, safe persistence, and resolution contracts without exposing plaintext command arguments.

**Blocked by:** 02 — Safely persist encrypted record changes; 03 — Resolve encrypted references safely across runtime consumers

**Status:** ready-for-agent

- [ ] Select a sole eligible Ed25519 agent key automatically; otherwise offer a fingerprint chooser. Handle no eligible keys, refusal, disconnection, and cancellation without arbitrary identity selection. Do not use vendor detection or agent startup/discovery.
- [ ] Author references using exact valid IDs and OpenSSH fingerprints. Omit the selector only when unambiguous under the per-ID resolution rule; otherwise persist explicit selection. Reuse the unchanged configuration discovery/schema and config-relative storage location.
- [ ] Provide masked exact UTF-8 value editing, including empty, multiline, whitespace, Unicode, and paste. Reject NUL rather than stripping it. Editor layout and additional conservative limits are delegated within the spec. Never unintentionally render/log plaintext or place it in argv; zeroize discarded secret buffers best-effort.
- [ ] Existing `(ID,key)` ciphertext is never silently overwritten: require replacement confirmation and warn updates affect all references to that record. The full reuse/update workflow and exhaustive replacement coverage belong to ticket 05.
- [ ] Persist encrypted storage before configuration. If storage fails, do not create a dangling reference; if config persistence fails, unused ciphertext may remain. Report actual surviving artifacts and partial completion accurately, including post-replacement sync failures; do not claim cross-file rollback.
- [ ] Keep human editing outside `PSST_TIMEOUT`; apply the agent-work deadline to save phases including queue time. Cancellation/stale results do not apply an old operation to a different draft; completed file changes are accurately reported, not promised undone.
- [ ] Use focused UI tests for masking/input, chooser/selector states, errors, cancellation, and stale results, plus representative PTY create → save → resolve workflows through a fake agent. Include exact empty/multiline/paste/NUL cases and no unintended plaintext in terminal output/logging/argv.
- [ ] Fault-inject both creation persistence stages and replacement/sync boundaries to prove ordering and accurate completion reports. Test terminal restoration and a usable UI after failure/cancellation; retain legacy editor/browser behavior and formatting/Clippy/tests.
- [ ] Do not add a standalone store browser, noninteractive management commands, bare subcommands, plaintext import/export management, or automated orphan cleanup.
