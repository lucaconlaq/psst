# 03: Complete file-provider interaction and privacy

**Source:** [Managed files](../../specs/02-managed-files.md)

**What to build:** Browse, author, reveal, and copy provider contents for managed files through existing interactive controls, safely and independently of destinations.

**Blocked by:** 02 — Manage file mappings in the Files dashboard

**Status:** done

Final accumulated verification: all seven criteria remain satisfied. Supplied independent static Spec PASS 30/30 and Rules PASS for introduced changes 30/30 against fixed base `20f516288522578052ef853a795a5dcb340fca0c`, followed by a fresh unchanged-source Linux Rust 1.88.0 configured-check run (266 tests passed), reaffirm this ticket. See [ticket 07](07-release-verification.md#final-local-reconciliation) for commands, immutable snapshot identity, reviews, and the outstanding native macOS release gate. Earlier pending statuses and dependent-ticket transitions below are historical, not current; no full-spec/release completion is claimed.

- [x] Ctrl+B uses existing provider browsing where supported; source selection and unknown-provider editing retain existing behavior.
- [x] Reuse local-storage authoring, keeping storage creation/replacement separate from selecting a reference and saving a file mapping. Preserve failure, cancellation, and partial-persistence semantics without linking stale results.
- [x] Provider authoring/replacement and browser record-deletion warnings count references from entries and managed files in the loaded configuration only; do not claim knowledge of other configurations.
- [x] Keep provider-browser record deletion separate from file-mapping deletion. File-mapping deletion never removes provider contents.
- [x] Reveal/copy resolves provider results only on request, never reads destinations, and never automatically fetches on dashboard display. Shell commands execute only when resolution is requested; manual editing reveals/copies draft text rather than fetching existing contents.
- [x] Preserve masking, short-lived reveal expiry, copy behavior, cancellation, and stale-result rejection across tab/editor changes. Diagnostics and warnings never expose resolved plaintext.
- [x] Verify browsing and local-storage authoring/linking with isolated fixtures and fake SSH agents, including shared warnings across kinds, browser deletion, failures, late results, and cancellation. Exercise requested shell resolution, draft-only editing, reveal expiry, and clipboard behavior with controlled helpers, UI tests, and representative PTY coverage.

## Implementation evidence (historical follow-ups)

- Preserved earlier tickets and reused the shared provider controls and loaded-configuration warning counts. Corrected authoring validation to distinguish destinations from environment names.
- UI coverage now exercises both mapping kinds for browser authoring/linking, replacement and configuration persistence failures, cancelled saves across dashboard changes, browser reveal/hide/expiry/cancellation, and requested editor resolution. Added destination-isolation and stale-preview checks; mixed-kind fixtures pin replacement/deletion warning counts.
- Fake-agent PTY coverage adds Files authoring, separate linking, and browser deletion/cancellation with unchanged destination contents and configuration retention. Shared replacement warnings include a managed file. Controlled clipboard success/failure tests cover both kinds in dashboard, reference, browser, manual-draft, and encrypted-draft views without unintended plaintext output.
- Historical direct-Cargo evidence (not the configured Mise verification): Linux Rust 1.88.0: `timeout 60s cargo +1.88.0 fmt --check` passed; `timeout 600s nix-shell -p gcc --run 'cargo +1.88.0 clippy --locked --all-targets -- -D warnings'` passed; `timeout 600s nix-shell -p gcc --run 'cargo +1.88.0 test --locked'` passed (213 tests, including 23 PTY tests). `git diff --check` passed. Logs: `/tmp/psst-ticket03-tests.log`, `/tmp/psst-ticket03-clippy.log`.
- Review follow-up against base `20f516288522578052ef853a795a5dcb340fca0c`: supplied spec review passed 18/18 with no findings; supplied rules review identified P2 RUST-11 at ticket line 24 because direct Cargo runs did not establish configured Mise verification. No new review was performed during this follow-up.
- Configured Mise verification on Linux via Nix GCC: `timeout 120s nix-shell -p gcc --run 'mise exec -- cargo fmt'`, `timeout 120s nix-shell -p gcc --run 'mise exec -- cargo fmt --check'`, `timeout 600s nix-shell -p gcc --run 'mise exec -- cargo clippy --locked --all-targets -- -D warnings'`, and `timeout 600s nix-shell -p gcc --run 'mise exec -- cargo test --locked'` all exited 0. Tests passed: 213, including 23 PTY tests. In the same Nix shell, `mise exec -- rustc --version` reported `rustc 1.88.0 (6b00bc388 2025-06-23)` and `mise exec -- cargo --version` reported `cargo 1.88.0 (873a06493 2025-05-10)`. Logs: `/tmp/psst-ticket03-mise-verification/{toolchain,fmt,fmt-check,clippy,test}.log`.
- RUST-11 verification evidence finding addressed; earlier direct-Cargo results retained above as historical evidence. Formatting left the pre-existing code diff unchanged, and `git diff --check` passed.
- Status remained in-progress during that follow-up. macOS verification deferred to the end by the user. No requirements or code changes, staging, or commits performed during that follow-up.

## Completion evidence

- Independent reviews against base `20f516288522578052ef853a795a5dcb340fca0c`: Spec **PASS**, 18/18 paths, no ticket-03 blockers (`/tmp/psst-subagent-5ODvhw/events.jsonl`); Rules **PASS**, 18/18 paths, no violations or blockers (`/tmp/psst-subagent-lNPjZj/events.jsonl`). These supersede the pending-review notes above; no self-review substituted. Their acceptance trace plus the fresh execution below satisfy all seven criteria.
- Final Linux rerun via `nix-shell -p gcc`: Rust `1.88.0 (6b00bc388 2025-06-23)`, Cargo `1.88.0`, GCC `16.2.0`. Passed `mise exec -- cargo fmt`, `mise exec -- cargo fmt --check`, `mise exec -- cargo clippy --locked --all-targets -- -D warnings`, and `mise exec -- cargo test --locked`. Limits: 30 seconds per toolchain command, 120/120/240/360 seconds for checks, 900 seconds for outer shell. **213 passed**, including **23 PTY tests**; zero failed, ignored, or filtered out. Log: `/tmp/psst-managed-files-03-final/verification.log`.
- Code/test diff unchanged by verification (`cmp` passed); working-tree and base-relative `git diff --check` passed. Only this ticket's completion record changed in this pass; no requirements changes, child agents, staging, commits, push, or merge.
- Ticket 03 is **done**; no ticket-03 blockers remain. No dependent ticket is newly unblocked: 07 retains its 03/06 blocker references and remains **blocked** by 06. macOS verification remains user-deferred to the final release gate, not waived or claimed complete; this is not full-spec release approval.
