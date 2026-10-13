# 03: Resolve encrypted references safely across runtime consumers

**Source:** [Agreed spec](../../../docs/specs/01-local-storage.md), including its linked idea and exclusions.

**What to build:** Users can inject, export, copy, or explicitly fetch their encrypted values through the exact `local-storage` source, with identity-safe selection, operation-local deduplication, and fail-stop command/export behavior. Use ticket 01's valid fixtures for end-to-end tests; authoring is not required to verify this slice.

**Blocked by:** 01 — Verify SSH-agent encrypted round-trips and define v1

**Status:** ready-for-agent

- [ ] Register the exact case-sensitive source `local-storage`, with provider-owned metadata. Preserve ordered string `source`/`value` configuration and discovery (`PSST_CONFIG`, nearest visible/hidden config). Read `.psst/local-storage/` beside the chosen config, never beside the caller's cwd.
- [ ] Parse exactly `secret-id` or `secret-id#SHA256:fingerprint` using OpenSSH SHA-256 public-key fingerprints and ticket 01's ID rules. Unqualified references require exactly one agent key with a stored value for that ID, not a key merely present elsewhere in storage. Qualified references select only that key for that entry.
- [ ] Missing keys/records, unsupported keys, ambiguity, agent refusal/disconnection/malformed responses, validation failures, and decryption/authentication failures are errors with no other-identity or plaintext fallback. Enforce 4 MiB reads, key/path matching, private local permissions, and symlink refusal before signing where applicable.
- [ ] Every `local-storage` resolution failure prevents wrapped-command launch and `.env` creation/replacement. CLI tests must cover each failure class with subprocess markers and filesystem assertions, including mixed successes and inherited variables. Other providers keep warning/omission behavior.
- [ ] Verify successful injection/export and exact empty/multiline/Unicode text under existing consumer rules: empty is a valid provider result, injection accepts it, and export still omits successful empty values. Copy failures must not launch a clipboard helper; retain existing empty-copy behavior.
- [ ] Deduplicate identical resolved `(ID,key)` fetches within one operation, including qualified/unqualified references resolving to the same record. Keep plaintext/derived keys operation-local, discard after use, and best-effort zeroize; there is no persistent cache. A single fetch and each TUI refetch start fresh operations.
- [ ] Count fake-agent requests for shared mappings, per-user values, and separate operations. Verify serialized signing, deadlines including queue time, active/queued cancellation, no retries/fallback, and usable subsequent operations. Preserve legacy provider concurrency, config order, individual errors, and the existing `op` queue policy.
- [ ] TUI opt-in fetches show per-entry failures and remain usable; preserve cancellation, stale-result rejection, short-lived deliberate reveals, and terminal restoration. Prove fresh refetches request the agent again rather than reuse a persistent secret cache.
- [ ] Test config-relative resolution from another caller directory, explicit selection with multiple loaded keys, per-ID zero/one/multiple matches, shared mappings, absent socket/key/value, malformed references/storage, oversize files, and symlinked storage. Pre-sign validation failures produce no signing requests; no private-key reads or `op` invocations occur.
- [ ] Prefer binary CLI and representative PTY boundaries, supplemented by focused operation/UI tests only where needed; any operation context is implementation discretion, not a new agreed public interface. Regress legacy discovery/providers, passthrough argv, copy helpers, export order/escaping, and concurrency. Follow repository guidelines and run formatting/Clippy/tests with fake-agent isolation.
