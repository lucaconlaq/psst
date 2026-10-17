# 01: Clarify pull direction while preserving compatibility

**Source:** [Save files to a provider](../../specs/03-save-files-to-provider.md)

**What to build:** Let users explicitly pull provider contents into managed files with `psst --files-pull [--yes]`, retaining `--write-files` as a compatibility alias without changing materialization behavior.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] Canonical pull and its legacy alias dispatch the existing pull operation. Help, usage errors, and user-facing documentation explain provider → local direction and the alias. Coordinate with push additions without making this ticket depend on them or advertising an unimplemented operation.
- [ ] Reject incompatible operation combinations rather than silently selecting a mode. Preserve native wrapped arguments after command dispatch or `--`, including arguments that look like psst flags; retain existing environment, copy/export, version, and TUI behavior outside incompatible managed-file combinations.
- [ ] Pull receives naming/alias changes only: preserve its staging, content normalization, empty-read handling, consent, destination safety, permissions, independent failures, and exit status semantics. Do not reuse its plaintext disk staging policy for future push.
- [ ] Exercise existing pull scenarios under both spellings at the binary boundary, including empty sets, unchanged files, replacement approval/refusal, `--yes` safety checks, provider failures, trimmed `op` fetches, and providers rejecting empty reads. Cover direction-specific help, invalid combinations in either option order, wrapped arguments, and native argv preservation.
- [ ] Run relevant parser, binary, materialization, and consumer regressions with isolated configurations and fake providers; record verification evidence. Keep Rust 1.88 compatibility and the existing green test suite.
