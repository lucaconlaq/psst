# 04: Support bounded exact subprocess transfers without changing fetch behavior

**Source:** [Save files to a provider](../../specs/03-save-files-to-provider.md)

**What to build:** Make the subprocess boundary capable of exact raw provider reads and secret-bearing stdin transfers, with deterministic verification against fake executables, while existing user-visible fetch behavior remains unchanged. This is the small prerequisite for restricted `op` updates, not provider support by itself.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] Preserve the existing trimmed-output fetch entry point and its size, UTF-8/NUL, exit-status, timeout, no-controlling-terminal, and subprocess-group cleanup semantics. Do not change provider empty-read policies or downstream environment/copy/export/pull behavior.
- [ ] Add bounded exact-output and stdin-payload capability beside the existing behavior, reusing process setup/cleanup where practical. Preserve empty output, Unicode, whitespace, and newlines when raw output is requested. Keep payloads out of argv and diagnostics; stdin must close correctly. Internal interfaces and precise buffer organization remain implementation-owned, not new public extension contracts.
- [ ] Bound input and captured output/working buffers, keep owned secret-bearing buffers zeroizing, suppress secret-bearing stderr, and report content-free errors. Handle a child producing output while consuming input without pipe deadlocks; do not use temporary plaintext files or detached payload-writer tasks.
- [ ] Timeout, cancellation, output overflow, invalid output, broken stdin, spawn failure, and unsuccessful exit clean up owned subprocess work, descendants, and buffers. Retain the existing positive `PSST_TIMEOUT` convention. Expose enough outcome evidence for the caller to report potentially uncertain remote writes rather than promising rollback; provider-specific reporting belongs to ticket 05.
- [ ] Test the real subprocess boundary with isolated fake executables: exact empty/multiline/Unicode output, stdin payload equality and EOF, simultaneous input/output pressure, limits at/over boundaries, early exit/broken pipe, suppressed secret stderr, timeout, cancellation, and descendant cleanup. Assert captured argv/errors contain no secret payload and no plaintext staging appears. Inspect ownership to verify cancellation zeroization rather than inferring it solely from files.
- [ ] Run existing provider/robustness/consumer regressions to prove the behavior-preserving part of this prerequisite. Record evidence; no ordinary test may authenticate to a real provider or use personal secrets.
