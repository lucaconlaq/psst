# Save files to a provider

Status: Agreed

## Executive summary

Allow explicitly saving a configured managed file's local contents back to its source storage: the reverse of materializing managed files in [spec 02](../specs/02-managed-files.md). Preserve local edits in the source provider, including project-local encrypted storage; surviving deletion of the entire project is not required.

## Notes

- This captures the reverse-update direction mentioned as future work in [idea 02](02-managed-files.md).
- Originally, provider support and the save trigger were unspecified; the decisions below resolve them.

## Design tree

- Q1 — Input scope: managed files only. Arbitrary-file import is excluded from this version.
- Q2 — Save initiation: explicit user action only. Automatic write-back is excluded.
- Q3 — Preservation goal: write back to the file's source storage. Project-local encrypted `.psst` storage is acceptable; the user expects it may be committed to Git. External backup and survival of whole-project deletion are not requirements.
- Q4 — Writable providers: user prioritizes “local file,” wants manual values updated in JSON, and wants `op` if its CLI permits editing. Exact local-source meaning and safe `op` scope remain unresolved. Vault, Terraform, shell, and derived OTP writes are not requested.
- Q5 — User interface: CLI only; TUI saving is excluded.
- Q6 — Selection scope: all configured managed files only; single-file/subset selection is excluded.
- Q7 — “Local file” means existing encrypted local storage, not a new plaintext-file provider.
- Q8 — Include restricted `op` updates: existing text/concealed fields only in safely round-trippable items; preserve other fields and metadata. Reject passkey-bearing items, attachments, derived OTP sources, unsupported/ambiguous targets, and creation of items or fields.
- Q9 — One default-no batch confirmation before source changes; `--yes` approves unattended updates. Show paths, providers, and shared-reference warnings, never contents. Refusal performs no writes.
- Q10 — Best effort: missing files, unsupported providers, and independent failures do not prevent valid saves. Any failure yields nonzero status; completed writes remain, with no cross-provider rollback.
- Q11 — Naming unresolved: user finds `--write-files` versus `--save-files` confusing; do not adopt that pair without a clearer direction distinction.
- Q12 — Preserve exact UTF-8, including empty files, whitespace, and newlines. Reject NUL, binary data, inputs over 4 MiB, and stricter provider-limit violations (local storage: 1 MiB).
- Q13 — Literal paths resolve beside the selected configuration; absolute/outside paths remain allowed. Refuse symlinked files/ancestors, nonregular files, and configuration/storage artifacts. Reads do not alter local files or permissions; 0600 is not required for input files.
- Q14 — Group by resolved provider target: identical contents coalesce; differing contents fail that target, never last-writer-wins. Unrelated targets continue. Combine different fields in the same `op` item safely.
- Q15 — Canonical commands: `psst --files-pull` (provider → local) and `psst --files-push` (local → provider), with `--yes` for consent. Keep `--write-files` as a compatibility alias for pull. Exclude mutually incompatible operation combinations.
- Q16 — Approval permits overwriting the latest value of the intended source target, even if it changed after planning. Preserve unrelated data where possible; do not describe this as stale-source conflict rejection. This supersedes the recommended rejection policy, not existing storage integrity requirements.
- Q17 — Fail a target if its local input changes after planning: revalidate file identity, path safety, and exact contents immediately before persistence. Never modify the local input.
- Q18 — Local-storage updates require an existing record and one unambiguous matching eligible SSH-agent key under existing reference rules. Missing records/keys and ambiguity fail individually. Creation and alternate-key selection are excluded.
- Q19 — Reload latest source state and overwrite only approved targets, preserving unrelated encrypted IDs, `op` fields, and configuration mappings. Retain locks/integrity checks; a final local race may fail, while `op` retains its remote race. Removed, retargeted, or newly unsafe targets fail.
- Q20 — Commit valid manual-file values together in one atomic configuration save; preserve environment entries and unrelated mappings. A save failure affects that group, independently of completed provider writes.
- Q21 — Exact raw stored-value matches are unchanged, omitted from confirmation and writes. Compare manual literals, decrypted local records, and raw `op` fields, not normalized fetch output. No changes means no prompt; missing/unsupported targets remain failures.
- Q22 — Memory-only snapshots in zeroizing buffers with a total batch cap; no plaintext staging. Never put secret contents in diagnostics or argv. Cancellation cleans owned buffers/work, retains completed writes, and reports interrupted remote writes as potentially uncertain.
- Q23 — Cap aggregate planned local-file plaintext at 64 MiB before deduplication. Exceeding the cap aborts before writes; no disk fallback. Per-file/provider limits and bounded provider working buffers also apply.
- Q24 — Preserve existing pull content semantics; rename/alias only. Document that `op` fetch trims output and some providers reject empty reads, so exact push does not imply byte-exact push→pull round trips.
- Q25 — Report every file as saved, unchanged, or failed, explicitly identifying partial completion, uncertain durability, or uncertain remote outcomes. Any failure, refusal, or cancellation is nonzero. Empty file sets succeed with guidance. Report all files affected by coalesced/grouped targets. Dry-run, JSON, watch, and subset modes are excluded.
- Q26 — Require isolated CLI/provider/agent tests, formatting, Clippy, the full suite, and Linux/macOS verification. Cover exact text, path/input changes, targeted latest-source overwrites, preservation of unrelated data, shared targets, unsupported/unsafe `op` items, consent, caps, aliases, cancellation, and partial/uncertain outcomes. Real provider checks require explicit opt-in; existing crypto release limitations remain.

## Open questions and investigations

- Verified: managed-file materialization is implemented; reverse-saving is not. Only `local-storage` currently exposes authoring. Other providers have no psst write interface; manual values live in configuration, and derived outputs are not generally reversible.
- Verified: local-storage ciphertext lives beside the selected configuration in `.psst`; keeping the SSH key alone cannot recover deleted ciphertext.
- Verified: managed-file resolution permits up to 4 MiB of UTF-8 without NUL; local-storage authoring accepts at most 1 MiB. Shared references may be affected by replacement.
- Verified from [1Password CLI item reference](https://developer.1password.com/docs/cli/reference/management-commands/item/): `op item edit` can edit fields and accept JSON over stdin. Sensitive assignments in argv are discouraged. JSON templates can overwrite passkeys, so unrestricted item rewriting is unsafe. Safe field scope and concurrency guarantees need resolution; no live provider access was performed.
- No unresolved design questions or investigations. The user confirmed the complete design.
- Implementation must fail closed for `op` items whose safe round-trip eligibility cannot be established; successful CLI editing in general is not proof that an arbitrary item is safe.
- Internal module/type choices remain implementation details, not additional product requirements.

## Agreement

The user explicitly confirmed Q1–Q26 and the shared summary: CLI-only push of all configured managed files to existing encrypted local-storage records, manual configuration values, and restricted safe `op` fields. Canonical pull/push flags distinguish direction while preserving the old pull alias. Batch consent permits targeted overwrites of latest source values, with exact-text guarded inputs, memory-only bounded planning, independent results, and explicit partial/uncertain outcomes. Existing pull normalization and crypto limitations remain. Excluded branches are recorded above.

This approval authorizes the idea only; spec creation and implementation require separate requests.
- Verified from [Edit items](https://www.1password.dev/cli/item-edit/): `op item edit` cannot edit SSH keys. The reviewed edit documentation exposes no conditional-write flag; a final reread cannot by itself guarantee race-free remote updates.
- Existing-document discrepancy, not changed here: spec 02 says `psst write-files`, while implementation uses `psst --write-files [--yes]`. Its referenced verification ticket is absent from this checkout.
