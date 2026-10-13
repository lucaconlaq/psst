# 06: Delete mappings with optional, identity-safe ciphertext removal

**Source:** [Agreed spec](../../../docs/specs/01-local-storage.md), including its linked idea and exclusions.

**What to build:** Deleting a `local-storage` environment mapping offers a safe choice to keep or remove its encrypted record, without accidentally deleting another user's value. This slice can use pre-existing encrypted fixtures and does not depend on TUI creation/update.

**Blocked by:** 02 — Safely persist encrypted record changes; 03 — Resolve encrypted references safely across runtime consumers

**Status:** ready-for-agent

- [ ] Ask whether to remove the encrypted record too, defaulting to keeping ciphertext. Warn about known shared references in the loaded configuration without claiming repository-wide reference discovery.
- [ ] Remove only a record explicitly selected by the reference or unambiguously resolved for that ID. Never delete all users' values for an ID; unresolved identity keeps ciphertext while still allowing safe mapping removal. Selection is not arbitrary and has no other-identity fallback.
- [ ] Persist configuration removal before any ciphertext removal. If config persistence fails before replacement, do not remove ciphertext; handle post-replacement errors according to the actual persisted outcome and accurately report partial completion. Failed storage deletion leaves ciphertext, not a rollback claim.
- [ ] Preserve unrelated IDs and all other users' records. Use per-key locking, loaded-content conflicts, private permissions, format/size validation, and symlink refusal for optional storage mutation; existing references to a deliberately removed record subsequently fail safely.
- [ ] Cover cancel/default-keep/explicit removal, qualified/unqualified references, missing or ambiguous identity, known shared mappings, malformed storage, lock contention, concurrent edits, and symlink paths. No automatic orphan cleanup or unrelated record deletion is allowed.
- [ ] Fault-inject both deletion persistence stages and replacement/sync boundaries. Assert order, actual surviving configuration/ciphertext, truthful partial-completion messages, and no silent clobbering or cross-file atomicity promise.
- [ ] Use exhaustive focused UI states and representative fake-agent PTY mapping-deletion workflows. Verify other users still resolve their values, a usable UI after failure/cancellation, stale-result safety, no unintended plaintext exposure, and terminal restoration.
- [ ] Preserve existing deletion behavior for other providers and repository formatting/Clippy/tests. If identity work contacts the agent, use the agreed phase deadline/cancellation rules with no retries/fallback and no persistent secret cache.
