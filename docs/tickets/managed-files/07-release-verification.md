# 07: Document usage and complete release verification

**Source:** [Managed files](../../specs/02-managed-files.md)

**What to build:** Provide accurate managed-file usage guidance and evidence that the complete integrated behavior meets the spec on supported platforms.

**Blocked by:** 03 — Complete file-provider interaction and privacy; 06 — Verify cancellation and adversarial materialization failures

**Status:** blocked

- [ ] Update user-facing usage/help for tagged configuration compatibility, Files controls, explicit psst write-files [--yes], mapping save versus materialization, batch consent, outcomes, partial failures, and unchanged permission errors.
- [ ] Explain plaintext destinations/staging, crash-leftover staging and manual inspection/removal, outside-root paths not being sandboxed, and trusted shell execution. Refusal prevents destination writes, not side effects already performed by providers.
- [ ] Preserve exclusions: no reverse sync, templating/interpolation, binary/dedicated downloads, automatic materialization or destination cleanup, subset/dry-run/watch/JSON modes, local-content viewer/comparison, automatic crash recovery, or metadata preservation beyond the agreed replacement contract.
- [ ] Audit the spec's complete acceptance matrix against evidence from tickets 01–06; close uncovered cases without treating POC checks as implementation evidence. Verify an integrated author/save/materialize flow and environment compatibility with isolated fixtures.
- [ ] Review path validation, alias/protected-artifact handling, conflict rechecks, persistence, and cleanup against the safety contract; passing happy-path tests alone is insufficient. Record review evidence and resolve findings before completion.
- [ ] Run configured formatting, Clippy, and the full test suite with pinned Rust 1.88.0 on supported Linux and macOS environments, including managed-file acceptance coverage. Record commands, platform results, and evidence; unavailable platform checks remain outstanding rather than being claimed complete.
- [ ] Normal tests require no real authentication or personal secrets. Any optional real-provider checks require explicit authentication consent and disposable destinations.
- [ ] Retain existing local-storage release limitations and outstanding gates; managed-files completion does not grant that provider production approval. Internal mechanisms and proposed testing seams remain bounded implementation discretion, not new product contracts.
