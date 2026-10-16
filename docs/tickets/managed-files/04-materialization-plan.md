# 04: Validate destinations and stage a bounded materialization plan

**Source:** [Managed files](../../specs/02-managed-files.md)

**What to build:** Turn configured managed files into a safe, bounded plan with privately staged provider results and individual outcomes, without writing destinations. This is an independently verifiable internal slice, not a new dry-run interface.

**Blocked by:** 01 — Preserve mixed configurations and isolate environment operations

**Status:** blocked

- [ ] Before any fetch, reject configuration-wide destination aliases and destinations targeting psst configuration files, their lock files, or .psst storage; stop the entire run rather than planning a subset.
- [ ] Interpret paths literally. Resolve relative destinations beside the defining configuration regardless of caller cwd; allow absolute/outside paths subject to safety checks. Alias-detection details remain implementation discretion.
- [ ] Resolve files sequentially through existing providers with their configuration working directory, timeouts, normalization, and stricter bounds. Enforce a 4 MiB UTF-8 result limit and shared NUL rejection; preserve provider-specific successful/rejected empty behavior.
- [ ] Stage successful results as 0600 files in a private 0700 system-temp directory outside destinations. Add no trimming, newline, interpolation, or other transformation; successful empty results stage empty files.
- [ ] Bound planning metadata and retain plaintext on disk rather than keeping every resolved result in memory. Owned staging cleans up on normal completion, error, and dropped/cancelled work.
- [ ] Plan new, unchanged, differing, and failed destinations without modifying them. Refuse symlinked destinations/ancestors and nonregular targets. Only identical regular 0600 files qualify as unchanged; identical contents with other permissions fail without repair.
- [ ] Preserve destination snapshots sufficient for later conflict checks and identify all valid replacements for batch consent. Individual planning/resolution errors, including local-storage errors, do not discard independent valid plans. Snapshot and cleanup mechanisms remain implementation choices.
- [ ] Verify no fetch after failed preflight using provider markers; exercise aliases/protected artifacts, literal/relative/absolute/outside paths, symlinks/nonregular targets, unchanged permissions, and private staging.
- [ ] Use controlled fixtures for every existing provider type: verify sequential execution, cwd independence, normalization, empty results, NUL/invalid UTF-8, timeout, exact/over-limit results, stricter provider bounds, unknown providers, and isolated local-storage failure. Prefer existing test seams; any narrow plan/commit fault seam is a testing proposal, not a required public API.
