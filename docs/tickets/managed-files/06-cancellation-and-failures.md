# 06: Verify cancellation and adversarial materialization failures

**Source:** [Managed files](../../specs/02-managed-files.md)

**What to build:** Make interruption, destination races, persistence failures, and large batches demonstrably safe, fixing gaps exposed by adversarial verification.

**Blocked by:** 05 — Materialize files with batch consent and safe persistence

**Status:** blocked

- [ ] Ctrl+C during resolution, waiting for consent, and between destination writes exits unsuccessfully, cleans staging and unfinished destination temporaries, and retains completed writes. Waiting for input and provider work must not prevent cancellation cleanup.
- [ ] Deterministically mutate destinations and ancestors between planning and commit, including newly appeared/replaced targets and symlink changes. Verify refusal without clobbering changed targets and successful independent writes.
- [ ] Inject staging, temporary-write, file-sync, replacement, and directory-sync failures through the narrowest useful test seam. Verify actual surviving bytes, temporary cleanup, failure status, and precise pre-/postreplacement reports, including uncertain durability.
- [ ] Exercise cancellation/failure with local-storage and subprocess fixtures without real authentication. Preserve existing environment-operation stop-on-local-storage-failure semantics while materialization remains best effort.
- [ ] Exercise many files and near-limit results, verifying private on-disk staging outside destinations, bounded planning rather than aggregate plaintext retention, and no fetch beyond failed configuration-wide preflight.
- [ ] Assert normal/error/refusal/cancellation runs leave no abandoned staging or unfinished destination temporaries and leak no contents in diagnostics. Do not promise cleanup after crashes or add startup sweeping/recovery.
- [ ] Keep tests deterministic and focused on outcomes and filesystem state rather than mandating internal types or helper boundaries. Record verification evidence and any platform-specific coverage limitations for the release gate.
