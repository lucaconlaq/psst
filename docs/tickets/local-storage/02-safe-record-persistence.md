# 02: Safely persist encrypted record changes

**Source:** [Agreed spec](../../../docs/specs/01-local-storage.md), including its linked idea and exclusions.

**What to build:** Independently verifiable creation, replacement, and removal of encrypted `(ID, public key)` records, protecting concurrent edits and accurately reporting what survived a failed save. Reuse ticket 01's agent/envelope contracts; do not add noninteractive management commands.

**Blocked by:** 01 — Verify SSH-agent encrypted round-trips and define v1

**Status:** ready-for-agent

- [ ] First add the smallest behavior-preserving persistence prefactor needed to distinguish failure before replacement from failure after replacement/directory sync. Cover existing configuration and `.env` atomic-write behavior with regressions; do not invent cross-file transactions or change legacy semantics.
- [ ] Store one JSON file per public key under `.psst/local-storage/` beside the selected configuration, using ticket 01's hash-safe filenames. Preserve unrelated IDs and other users' files; IDs never become paths.
- [ ] Read/write at most 4 MiB per key file. Strictly validate the complete applicable format/key/path metadata before signing; malformed/duplicate/unknown-version/oversized data must not be silently replaced. Verify these failures issue no signing requests.
- [ ] Refuse symlinked storage paths, including relevant storage directories, record and lock paths. Use private local directory/file permissions and verify them. Test symlink refusal on reads, writes, and removal rather than relying on the existing atomic writer alone.
- [ ] Protect per-key mutations with locking, loaded-content conflict detection, and synced whole-file atomic replacement. Reject lock contention and concurrent same-key changes without clobbering; preserve every unrelated record. Validate serialized size before replacement.
- [ ] Test create, replacement with fresh salt/nonce, and targeted removal followed by successful decryption of surviving records. Include multiple IDs, multiple users, missing files, conflicts, permissions, and whole-file atomicity.
- [ ] Inject write, replacement, and sync-boundary failures; report whether content actually changed without claiming rollback or complete success. Ensure callers can honor storage-before-config creation and config-before-ciphertext deletion, including completed changes on cancellation; full two-stage workflow verification belongs to tickets 04 and 06.
- [ ] Use fake agents only, apply ticket 01's save-phase deadline/cancellation and secret-lifetime rules, and keep existing configuration/export tests plus repository formatting/Clippy/tests green.
