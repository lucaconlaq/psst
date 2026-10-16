# 01: Preserve mixed configurations and isolate environment operations

**Source:** [Managed files](../../specs/02-managed-files.md)

**What to build:** Load and preserve environment entries and managed files together while existing environment operations continue to act only on entries.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] Retain insertion-ordered flat JSON with string source/value fields. Missing type loads as env; saves emit explicit env/file tags. Reject malformed or unknown tags and duplicate keys, including cross-kind collisions.
- [ ] Use the destination key as the managed file's sole identity. Preserve existing environment-name validation; reject empty destinations and control characters without expanding home or environment variables. TOKEN and ./TOKEN can coexist without overwriting either mapping.
- [ ] Preserve both kinds through existing entry editing, renaming, and deletion, including unknown-provider mappings remaining editable. Mapping changes do not alter destination files or provider contents.
- [ ] Preserve configuration discovery (PSST_CONFIG, psst.json, legacy .psst.json), provider identifiers, configuration size/NUL validation, loaded-content conflicts, locking, atomic persistence, and pre-/postreplacement failure behavior.
- [ ] Wrapped commands and --write exclude file mappings from resolution, injection, and export and never materialize them. Existing environment failure semantics remain, including local-storage failures stopping launch/export.
- [ ] Verify legacy and mixed round trips, invalid schemas/paths/names, collisions, discovery, persistence conflicts/faults, and content-free diagnostics. Executable tests use a file-provider side-effect marker to prove environment operations never fetch files.
- [ ] Keep the full configuration independent of UI selection; do not adopt the POC's active/inactive map swapping. Internal representation remains implementation discretion, with regression coverage for any necessary prefactor.
