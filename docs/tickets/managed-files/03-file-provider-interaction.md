# 03: Complete file-provider interaction and privacy

**Source:** [Managed files](../../specs/02-managed-files.md)

**What to build:** Browse, author, reveal, and copy provider contents for managed files through existing interactive controls, safely and independently of destinations.

**Blocked by:** 02 — Manage file mappings in the Files dashboard

**Status:** blocked

- [ ] Ctrl+B uses existing provider browsing where supported; source selection and unknown-provider editing retain existing behavior.
- [ ] Reuse local-storage authoring, keeping storage creation/replacement separate from selecting a reference and saving a file mapping. Preserve failure, cancellation, and partial-persistence semantics without linking stale results.
- [ ] Provider authoring/replacement and browser record-deletion warnings count references from entries and managed files in the loaded configuration only; do not claim knowledge of other configurations.
- [ ] Keep provider-browser record deletion separate from file-mapping deletion. File-mapping deletion never removes provider contents.
- [ ] Reveal/copy resolves provider results only on request, never reads destinations, and never automatically fetches on dashboard display. Shell commands execute only when resolution is requested; manual editing reveals/copies draft text rather than fetching existing contents.
- [ ] Preserve masking, short-lived reveal expiry, copy behavior, cancellation, and stale-result rejection across tab/editor changes. Diagnostics and warnings never expose resolved plaintext.
- [ ] Verify browsing and local-storage authoring/linking with isolated fixtures and fake SSH agents, including shared warnings across kinds, browser deletion, failures, late results, and cancellation. Exercise requested shell resolution, draft-only editing, reveal expiry, and clipboard behavior with controlled helpers, UI tests, and representative PTY coverage.
