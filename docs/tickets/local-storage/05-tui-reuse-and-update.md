# 05: Reuse and update encrypted records through the TUI

**Source:** [Agreed spec](../../../docs/specs/01-local-storage.md), including its linked idea and exclusions.

**What to build:** Users attach additional environment mappings to an existing encrypted record without duplicating ciphertext, and deliberately update that record with clear shared-reference consequences. Extend ticket 04's TUI workflow using the existing safe storage and operation contracts.

**Blocked by:** 04 — Create encrypted secrets through the masked TUI

**Status:** blocked

- [ ] Reuse existing `(ID,key)` records when authoring mappings without duplicate records or ciphertext rewrites. Newly authored selectors remain omitted only when per-ID resolution is unambiguous; multiple loaded keys and users never cause silent value substitution.
- [ ] Require explicit confirmation before replacement and warn that an update affects all references to that `(ID,key)`. Known-reference details are limited to the loaded config; do not claim repository-wide discovery.
- [ ] Mask editing and preserve exact UTF-8 text, empty/multiline/whitespace/Unicode values, rejecting NUL including paste. Maintain the no-plaintext-logs/argv contract, best-effort zeroization, and fresh-operation fetch semantics; do not persist a plaintext/derived-key cache.
- [ ] Replacement produces fresh salt/nonce and keeps all unrelated IDs and other users' records. Existing and newly reused mappings resolve the new value; mappings to another user's same ID remain unchanged.
- [ ] Enforce safe per-key locking/conflicts and size/path validation. Retain the documented two-stage ordering whenever a mapping is also created/changed, and report actual partial completion, including post-replacement failures. A declined replacement makes no storage change.
- [ ] Test absent/ambiguous identities, agent rejection, timeout including queued signing, cancellation, stale results, lock contention, concurrent edits, and failed saves without retries, fallback, or unintended clobbering. Human editing has no agent deadline.
- [ ] Prove ciphertext reuse and shared-update effects with filesystem and runtime assertions. Use focused UI coverage for confirmations/masking/exact text/failure states and representative fake-agent PTY reuse/update workflows, including terminal restoration and UI usability after failures.
- [ ] Keep legacy providers/editors, source/reference editing, and formatting/Clippy/tests green. No automatic migration/rotation, recovery recipients, multi-recipient sharing, or new management CLI is introduced.
