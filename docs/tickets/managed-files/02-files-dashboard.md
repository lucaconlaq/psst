# 02: Manage file mappings in the Files dashboard

**Source:** [Managed files](../../specs/02-managed-files.md)

**What to build:** Browse and manage destination-to-provider mappings in a separate Files dashboard without materializing or removing destination files.

**Blocked by:** 01 — Preserve mixed configurations and isolate environment operations

**Status:** blocked

- [ ] Start on Entries. Dashboard Tab switches Entries/Files and preserves each view's filter and selection; contextual Tab behavior in editors, browsers, and dialogs remains unchanged.
- [ ] Use actual Ratatui Tabs attached to the list's top border, with padded Entries/Files labels, dot divider, and contrasting filled active tab. Advertise Tab in the footer; put concise count/filter titles such as Files · 1 on the bottom border, not materialization instructions.
- [ ] Files shows Destination, Source, Reference, and Contents plus a selectable + New file row, with usable empty/filtered states. Ctrl+N or Enter on creation opens a draft; Enter on a mapping edits it.
- [ ] Reuse the provider editor/source selector and existing filter, selection, help, discard, and quit conventions. Label the first file field Destination. Do not introduce POC placeholder defaults.
- [ ] Ctrl+S saves only the mapping and explains the separate psst write-files step. Creation, editing, renaming, and deletion preserve the other kind and reject cross-kind key collisions.
- [ ] Ctrl+D confirms mapping deletion without a ciphertext-removal toggle for files. Deleting/renaming retains destination files and provider contents; existing entry deletion behavior remains intact.
- [ ] Keep contents masked and perform no automatic fetching or destination reads. Switching views clears revealed values and invalidates pending results; retain generation/cancellation safeguards while adapting shared controls.
- [ ] Verify mixed-view persistence, validation and save failures, mapping-only changes, navigation and contextual Tab behavior with UI state/TestBackend tests and representative PTY flows. Check help, supported small-terminal layouts, discard, quit, and terminal restoration. Provider-specific interaction completion belongs to ticket 03.
