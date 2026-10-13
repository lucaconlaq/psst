# Local storage

Status: Agreed

## Executive summary

Add a new psst provider alongside Vault, 1Password, Terraform outputs, shell, and the other existing providers. The captured proposal is storage for secrets encrypted with the user's Ed25519 key, available via an SSH agent, supporting multiple users sharing the same repository, potentially with each user encrypting their own secrets. Accepted direction: repository-local encrypted secrets in a shared `.psst/` directory, with different users' secrets identified by their public keys, and private-key operations exclusively through the SSH agent. Each stored secret is unique to (secret ID, public key) and may be referenced by multiple environment entries. Existing config files hold references; `.psst/` beside the config holds encrypted data. Users can specify a public key in psst.json to resolve identity conflicts. References may omit the public key only when exactly one agent key matches a stored value for that ID. Identity and decryption failures are errors with no fallback. The accepted reference format is `secret-id` or `secret-id#SHA256:fingerprint`. The provider is named `local-storage`, uses standard SSH-agent access with 1Password as the required tested integration, and includes TUI value creation/update and deletion prompts. The user accepts signature-derived encryption's agent-access threat boundary. Accepted encryption uses verified repeatable Ed25519 signatures, HKDF-SHA256, and AES-256-GCM with fresh per-write salt/nonce and authenticated metadata. Storage is one JSON file per public key, containing separate entries by secret ID. Resolution failures stop command execution/export for this provider; recovery depends on preserving the SSH key, with manual migration only initially. Operational rules are settled: conflict-safe atomic saves with explicit partial-failure handling, masked TUI editing, operation-local caching only, strict bounded storage validation, and Unix SSH_AUTH_SOCK access with PSST_TIMEOUT deadlines. Release requires automated and opt-in 1Password compatibility tests plus security/recovery documentation; production readiness additionally requires independent crypto/format review. Exact format/library/UI details are delegated within these constraints. The user confirmed the complete design in Q28; implementation is not authorized by this agreement.

## Notes

- Historical captured proposal: all users' encrypted secrets would live in `.psst`, in the same file as the psst JSON configuration. Superseded by Q4/Q17: separate per-key JSON files under `.psst/local-storage/` beside the reference config.
- Confirmed in Q11: store the Ed25519 SSH private key as a 1Password SSH Key item and use it exclusively through 1Password's SSH agent, not by fetching a separate encryption key with op.
- The settled storage source name is `local-storage`; the placeholder example below is historical captured context. A secret entry would reference an identifier within the new storage:

```json
{
  "API_KEY": {
    "source": "__new__name__",
    "value": "id_inside_the_storage"
  }
}
```

## Motivation

Keep encrypted secrets alongside repository configuration without requiring a hosted secret manager, exposed as an additional psst provider. Acceptance criteria are recorded in Q20–Q27.

## Goals and non-goals

- Captured proposal: SSH-agent-backed encryption and multiple repository users.
- Confirmed: per-(ID, public key) values, reusable references, shared repository storage, and agent-only SSH private-key access.
- Accepted: signature-derived-key threat boundary, standard agent protocol with 1Password as required integration, and TUI-only initial storage management.
- Accepted: versioned HKDF/AES-GCM format, per-key JSON files, fail-stop runtime behavior, manual migration, and no automatic rotation/recovery recipients/multi-recipient sharing initially.
- Accepted: conflict-safe atomic persistence with reported partial completion, masked exact-text editing, bounded operation-local secret lifetime, strict format/filesystem validation, and experimental release until independent crypto/format review.
- Exact encoding/library/editor details are explicitly delegated within Q27's constraints.
- This discussion does not authorize implementation.

## Repository findings

- `README.md` and `src/config.rs`: configuration files are `psst.json` or `.psst.json`, not `.psst`; the root currently maps environment names directly to string source/value entries.
- `src/config.rs`: saves validate the complete map and use conflict detection, a cooperative lock, and atomic replacement. Embedded storage would require explicit schema and persistence design.
- `src/providers/mod.rs`: providers currently receive only a reference and config directory; embedded storage may require additional context.
- `Cargo.toml`: no SSH-agent or encryption library is currently declared.
- `src/main.rs` and `src/operations.rs`: current fetch failures warn and are omitted from injection/export; commands still launch and inherited variables remain. Q18 addresses run-level behavior separately from Q8.
- `src/cli.rs`: first unrecognized argument starts the wrapped command; bare management subcommands could break passthrough.
- `src/main.rs` and `src/operations.rs`: fetch errors currently warn and are omitted from command injection / .env export; commands still launch and inherited variables remain. Q8's no-identity-fallback guarantee does not establish run-level failure behavior.
- `src/cli.rs`: the first unrecognized argument starts the wrapped command, so new bare storage subcommands could break existing passthrough behavior.

## Design tree

- Q1 — Unmet need [settled]
  - Prerequisites: none.
  - Original question clarification: the user means a new psst provider alongside existing providers, not a separate storage application or replacement for them.
  - Presented question: Should this provider resolve secrets from encrypted data stored in the same repository config, without requiring Vault, 1Password, or another external secret store?
  - Recommendation: yes; provide repository-local encrypted values through the existing provider model, with local key/recovery responsibilities.
  - Decision and rationale: settled. The user confirmed a new provider for repository-local encrypted secrets; it complements existing providers.
  - Q4 — Configuration placement [settled]
    - Prerequisites: Q1 and the user's `.psst/` clarification.
    - Presented question: Should the existing psst.json/.psst.json remain the environment-reference config, while only encrypted storage lives under .psst/?
    - Recommendation: yes; preserve discovery and the existing config schema, accepting two repository artifacts.
    - Decision and rationale: accepted. Keep existing config discovery and environment mappings; encrypted data lives in .psst/ beside the config.
- Q2 — Multiple-user semantics [settled]
  - Prerequisites: none.
  - Presented question: Should the initial design give each user their own private values for the same environment names, rather than distribute shared secrets to multiple recipients?
  - Recommendation: yes; follow the captured per-user direction, accepting that team-secret distribution is a different feature.
  - Decision and rationale: accepted per-user storage in a shared project .psst/ folder; different users can encrypt different secrets and are identified by public key. Automatic selection behavior and whether cross-user access is supported are not yet decided. This supersedes the captured same-config-file placement proposal.
  - Q5 — Logical secret references [settled]
    - Prerequisites: Q2.
    - Presented question: Should a provider reference be a logical secret ID shared across users, with each public-key identity supplying its own value for that ID?
    - Recommendation: yes; keep one shared environment mapping and separate encrypted values, at the cost of requiring an identity-selection rule.
    - Decision and rationale: each secret is unique to (ID, public key). Multiple environment mappings may reference the same stored secret. Whether a reference may omit the key remains open in Q7.
    - Q7 — Optional public-key qualifier [settled]
      - Prerequisites: Q5 and Q6.
      - Presented question: Should a reference contain a secret ID plus an optional public-key selector, with omission permitted only when exactly one agent key has a stored value for that ID?
      - Recommendation: yes; simple references for unambiguous cases and per-entry identity control for conflicts. Matching is per secret ID, not the folder as a whole.
      - Decision and rationale: accepted. Omission requires exactly one agent-key match for the referenced ID; explicit selection is per entry.
      - Q10 — Reference encoding [settled]
        - Prerequisites: Q7 and existing string-valued config schema.
        - Presented question: Should references be ID or ID#SHA256:fingerprint, with an OpenSSH SHA-256 fingerprint identifying the public key and # excluded from IDs?
        - Recommendation: yes; retain the existing string value schema and make each reference independently selectable.
        - Decision and rationale: accepted ID#SHA256:fingerprint format; ID-only references follow Q7. The delimiter is reserved and unavailable inside IDs.
  - Q6 — Identity selection [settled]
    - Prerequisites: Q2 and Q3.
    - Presented question: Should psst automatically select an agent key only when exactly one matches stored identities, and require an explicit public-key fingerprint when several match?
    - Recommendation: yes; avoid arbitrary first-key selection, but multi-key users need explicit selection.
    - Decision and rationale: the user chose public-key specification in psst.json when there is a conflict. This establishes config-based disambiguation; Q7 defines omission behavior and Q8 defines failure behavior.
    - Q8 — Fail closed on identity problems [settled]
      - Prerequisites: Q6.
      - Presented question: Should missing keys/values, unresolved identity conflicts, rejected agent operations, and decryption failures be errors, without falling back to another identity or plaintext storage?
      - Recommendation: yes; avoid injecting a different user's value. Preserve existing command error policy, subject to investigation before defining run-level behavior.
      - Decision and rationale: accepted. Missing keys/values, unresolved conflicts, rejected agent operations, and decryption failures are errors without identity or plaintext fallback.
- Q3 — Agent-only key access [settled]
  - Prerequisites: none.
  - Presented question: Should private keys remain exclusively behind the SSH agent, with psst never reading private-key files, even if this restricts supported agents or key types?
  - Recommendation: yes; preserve the agent trust boundary, but validate a suitable encryption construction before committing to Ed25519 support.
  - Decision and rationale: accepted; always use the SSH agent, never private-key files.
  - Q9 — Signature-derived encryption threat boundary [settled]
    - Prerequisites: Q3, public documentation investigation, and clarification Q11.
    - Original answer: unclear to the user; they expect encryption and decryption with the same key. No threat-boundary acceptance was inferred.
    - Presented clarification: Is it acceptable to derive a separate encryption key from a repeatable SSH-agent signature, rather than directly encrypt/decrypt with the SSH private key, accepting that a process which obtains this signature can retain the derived key?
    - Recommendation: yes, if agent-authorized processes are trusted; keep SSH private keys inside the agent, but acknowledge derived keys exist in psst memory and can be captured by a process with signing access. Warn against untrusted forwarding and recommend per-operation confirmation. Specific construction/dependency approval and independent review remain separate.
    - Decision and rationale: accepted via 'yes to all'. Agent-authorized processes can retain derived keys after the agent locks; document this and forwarding risk.
    - Presented question after Q11: Should psst derive its encryption key from a repeatable signature authorized through the 1Password SSH agent, accepting that an authorized process could retain the derived key even after 1Password locks?
- Q11 — Key storage in 1Password [settled]
  - Prerequisites: none; clarification of the user's key-storage requirement.
  - Presented question: Do you mean saving an Ed25519 SSH private key as an SSH Key item in 1Password and using it only through the 1Password SSH agent, rather than fetching a separate encryption key using op?
  - Recommendation: yes; retain the agent-only boundary. This does not itself resolve the separate need for a compatible encryption construction or Q9's threat boundary.
  - Decision and rationale: accepted. The SSH private key lives in 1Password and is used through its SSH agent. This does not yet accept signature-derived encryption or its threat boundary.
  - Q12 — Agent compatibility scope [settled]
    - Prerequisites: Q11.
    - Presented question: Should the provider use the standard SSH-agent protocol, with 1Password as the required tested integration, rather than depend on the op CLI or enforce 1Password-only agent detection?
    - Recommendation: yes; target 1Password while allowing compatible deterministic Ed25519 agents. Only explicitly tested agents would be advertised as supported.
    - Decision and rationale: accepted via 'yes to all'. Standard SSH-agent protocol, 1Password required tested integration, no op dependency; advertise only tested integrations.

- Q13 — Provider name [settled]
  - Prerequisites: confirmed new-provider scope and agent-only access.
  - Original recommendation ssh-agent was rejected; the user preferred local-encryption or a similar name.
  - Presented question: Should the exact persisted source name be local-encryption?
  - Recommendation: yes; express repository-local encrypted storage without vendor coupling. Changing it later requires compatibility handling.
  - Decision and rationale: originally accepted local-encryption via 'yes to all'; subsequently renamed to local-storage by explicit user request, including the provider name, storage directory, and planning documents.
- Q14 — TUI deletion prompt [settled]
  - Prerequisites: confirmed separate encrypted storage and reference config.
  - Original question proposed console CRUD, separate references, and no bare CLI subcommands. The user answered 'the user should be asked'; the triggering action is unclear, so none of those scope choices were inferred as accepted.
  - Presented clarification: Do you mean that when deleting an environment mapping, psst should ask whether to delete its stored encrypted secret too, defaulting to keeping it and warning if other mappings may use it?
  - Recommendation: yes as a tentative interpretation; keep by default and make consequences explicit. Console authoring scope still needs a separate answer after this clarification.
  - Decision and rationale: accepted specifically for TUI deletion of an environment mapping. Ask whether to also delete the encrypted stored secret, default to keeping it, and warn about other mappings that may use it. No non-TUI management scope was inferred.
  - Q15 — TUI authoring scope [settled]
    - Prerequisites: Q14 and confirmed local-storage provider.
    - Presented question: Should the initial feature include creating and updating encrypted stored values in the TUI, in addition to referencing existing values, with noninteractive storage-management commands excluded initially?
    - Recommendation: yes; make the feature usable end-to-end in psst without introducing plaintext CLI arguments or passthrough conflicts.
    - Decision and rationale: accepted via 'yes to all'. Initial storage management is in the TUI; noninteractive storage-management commands are excluded initially.
- Q16 — Encryption implementation and envelope [settled]
  - Prerequisites: Q9, Q12, and disposable test.
  - Recommendation: in-process verified repeatable Ed25519 signatures, HKDF-SHA256, and AES-256-GCM; versioned envelope, fresh random salt/nonce per write, authenticated ID/key/version metadata, and fixed-format domain-separated challenges.
  - Decision and rationale: accepted. Maintained Rust crypto primitives, not custom primitives or external plugins. Validate metadata/sizes before signing. Exact libraries are bounded implementation details subject to Rust 1.88 compatibility and security checks.
- Q17 — Per-key storage records [settled]
  - Prerequisites: Q4, Q5, Q13.
  - Original recommendation: one file per (ID,key), with hash-safe paths and visible metadata.
  - Decision and rationale: user chose one JSON file per key, with separate elements by ID inside, superseding per-secret files. Use hash-safe key paths under .psst/local-storage/; IDs never become paths. Metadata remains visible. Same-key edits need conflict handling.
- Q18 — Run-level provider failure [settled]
  - Prerequisites: Q8 and src/main.rs investigation.
  - Recommendation: local-storage resolution failures stop command launch / .env export; other providers retain existing behavior.
  - Decision and rationale: accepted. Copy fails normally; TUI shows per-entry errors and stays usable.
- Q19 — Recovery and rotation scope [settled]
  - Prerequisites: Q3, Q5, Q9, Q11, Q15.
  - Recommendation: preserve the SSH key in 1Password; manually migrate while the old key works; no automatic rotation, recovery recipient, or multi-recipient sharing initially.
  - Decision and rationale: accepted with consequences. Lost key means lost decryptability; rotation cannot revoke old history/captured derived keys. Migration creates new (ID,key) records and updates explicit selectors as needed.
- Q20 — Persistence and two-file operations [settled]
  - Prerequisites: Q4, Q14, Q15, Q17.
  - Presented question: Should per-key files use locking, loaded-content conflict detection, and atomic replacement, with storage saved first when creating references and config saved first when deleting?
  - Recommendation: yes; no cross-file transaction promise. Failed reference creation may leave an unused encrypted record; failed storage deletion leaves ciphertext. Explain partial completion and never silently overwrite concurrent edits.
  - Decision and rationale: accepted. Use locking, loaded-content conflict detection, and atomic replacement; storage-first creation and config-first deletion. Report partial completion; unused ciphertext is preferable to dangling references or silently overwritten changes.
- Q21 — TUI creation and value editing [settled]
  - Prerequisites: Q7, Q10, Q14, Q15.
  - Presented question: Should creation auto-select only a sole eligible agent key, otherwise show a fingerprint chooser, with masked value editing that preserves exact UTF-8 text, including multiline/empty values but excluding NUL?
  - Recommendation: yes; confirm replacement of existing (ID,key), reuse references without duplicating storage, and never log plaintext or put it in argv.
  - Decision and rationale: accepted. Auto-select a sole eligible key, otherwise choose by fingerprint; masked exact UTF-8 editing supports multiline and empty values, rejects NUL, and confirms replacement. Reuse existing records without duplication; no plaintext logs or argv.
- Q22 — Agent requests and caching [settled]
  - Prerequisites: Q12, Q16.
  - Presented question: Should identical (ID,key) fetches be deduplicated per operation, signing requests serialized with timeout/cancellation, and plaintext/derived keys never persistently cached?
  - Recommendation: yes; avoid duplicate prompts and approval races. Discard operation-local secrets after use, fresh operation for TUI refetches, best-effort zeroization without compromised-process guarantees.
  - Decision and rationale: accepted. Deduplicate identical (ID,key) fetches per operation, serialize signing with timeout/cancellation, and retain no persistent plaintext/derived-key cache. TUI refetch is a fresh operation; best-effort zeroization only.
- Q23 — File and format hardening [settled]
  - Prerequisites: Q16, Q17.
  - Presented question: Should key files be bounded to 4 MiB, reject malformed/duplicate/unknown-version data and mismatched key metadata, refuse symlinked storage paths, and use private local permissions while remaining safe to commit as ciphertext?
  - Recommendation: yes; validate before signing, verify signatures and creation repeatability, reject tampering. Metadata secrecy and replay/rollback protection for valid Git history are not promised.
  - Decision and rationale: accepted. Enforce 4 MiB per-key files, strict format/key validation, symlink refusal, private local permissions, signature verification and creation repeatability checks. Visible metadata and replay of valid history are accepted limitations.
- Q24 — Safe TUI references and deletion [settled]
  - Prerequisites: Q7, Q14, Q20, Q21.
  - Recommendation: newly authored references omit the fingerprint only if unambiguous; otherwise store the explicit selector. Editing stored values changes all references to that (ID,key), with a warning. Deletion targets only an explicitly selected/unambiguously resolved record, never every user's value; warn about known shared references, do not claim repository-wide reference discovery, and keep ciphertext when identity is unresolved.
  - Decision and rationale: accepted. Newly authored references omit selectors only when unambiguous. Warn that edits affect all references to the same record. Delete only a clearly selected record, keep ciphertext if identity is unresolved, and default to keeping ciphertext when removing mappings. Warn about known loaded-config references without promising repository-wide discovery.
- Q25 — Release and security acceptance [settled]
  - Prerequisites: Q12, Q16, Q18, Q20–Q23 and repository test/CI investigation.
  - Recommendation: require fake-agent/crypto/persistence/TUI/fail-stop regression tests, existing Linux/macOS CI and Rust 1.88 compatibility, consented real 1Password end-to-end tests on Linux/macOS, and setup/recovery/threat-boundary documentation. Require independent crypto/format review before advertising production readiness; until then explicitly experimental. Live-agent tests are opt-in, never automated against personal keys.
  - Decision and rationale: accepted. Require the proposed automated regression coverage, existing Linux/macOS targets and Rust 1.88 compatibility, opt-in real 1Password end-to-end checks on Linux/macOS, and security/setup/recovery documentation. Label experimental until independent crypto/format review; do not claim the toy feasibility test is such a review. Normal CI never contacts personal agents.
- Q26 — Agent connection and deadline [settled]
  - Prerequisites: Q12, Q22 and existing timeout investigation.
  - Recommendation: Unix SSH_AUTH_SOCK only initially, no agent autodiscovery/startup or forwarded-agent detection guarantee; use existing PSST_TIMEOUT (default 30 seconds) for each fetch/save agent-work phase including queued signing, not human editing time. Timeout/cancellation aborts that phase without retry/fallback; completed file changes follow Q20.
  - Decision and rationale: accepted as clarified above. Reuse PSST_TIMEOUT for agent-work deadlines; human editing has no such deadline. No automatic retry, agent startup/discovery, or forwarded-agent detection guarantee.
- Q27 — Bounded implementation discretion [settled]
  - Prerequisites: Q10, Q16, Q17, Q21, Q23.
  - Recommendation: delegate precise v1 JSON fields/challenge encoding, maintained compatible libraries, hash-safe filenames, conservative ID/input limits, and editor layout to implementation with tests and documented format. IDs are nonempty, case-sensitive exact strings, disallow #/control characters, never paths. No key types beyond deterministic Ed25519, metadata hiding, automated orphan cleanup, standalone store browser, import/export management, or automatic migration initially.
  - Decision and rationale: accepted. Delegate precise format fields/challenge encoding, compatible maintained dependencies, filenames, input limits and editor layout to documented/tested implementation within the agreed security boundaries. Apply the proposed ID rules and exclude all listed extensions initially; no standalone store browser.
- Q28 — Final shared-understanding confirmation [settled]
  - Prerequisites: Q1–Q27, all settled.
  - Recommendation: confirm this bounded design as agreed, without authorizing implementation.
  - Decision and rationale: confirmed. The user accepts Q1–Q27 and their explicit exclusions/delegations as the complete design. Mark the idea Agreed; implementation requires a separate request.

## Open questions and investigations

- Q1–Q28 are settled with their recorded boundaries, exclusions and explicit implementation delegation. No open design questions remain.
- Additional repository investigation: src/process.rs defines PSST_TIMEOUT with a 30-second default; src/providers/mod.rs already rejects NUL and uses zeroizing strings. CI tests/builds Linux and macOS (x86_64/aarch64); Cargo.toml requires Rust 1.88. These findings inform Q25–Q27 and are not user decisions.
- [1Password SSH Agent documentation](https://developer.1password.com/docs/ssh/agent/) confirms Ed25519 SSH Key items can be held in 1Password and used through its agent without exporting private keys. This verifies key custody/signing support, not encryption support. Authorization settings control approval duration; do not assume every signature always prompts.
- Public documentation investigation: [age README](https://github.com/FiloSottile/age#ssh-keys) explicitly says its built-in SSH-key support requires private-key files and does not support ssh-agent.
- [age-plugin-sshagent documentation](https://pkg.go.dev/github.com/unidoc/age-plugin-sshagent) describes deriving X25519 identities from deterministic Ed25519 agent signatures with HKDF, verifying signatures and testing repeatability. It explicitly warns that agent access permits permanent derivation-key capture, including through forwarding. These are documented implementation claims, not an independent security review or a selected dependency.
- Initial research did not contact the project agent, authentication services, or remote secret stores. Subsequently, the user explicitly authorized the live-agent experiment recorded below. Local SSH docs and cached relevant Rust sources were unavailable.
- Historical investigation state: feasibility initially had only a documented candidate. Q16 now settles the construction; Q25/Q27 explicitly delegate dependency selection and require compatibility/security checks during implementation before release.
- User-authorized disposable live-agent test succeeded: the single advertised Ed25519 key produced two identical, verified signatures over a domain-separated random challenge. Node built-in HKDF-SHA256 derived matching AES-256-GCM keys; encrypt/decrypt of `CIAO` succeeded and a modified authentication tag was rejected. No dependencies installed, project code changed, or key/ciphertext files written. This validates this agent/key's signing repeatability and a toy round-trip only, not a production format, independent security review, or acceptance of Q9/Q12. Agent implementation was not independently identified; the socket path alone does not prove 1Password custody.
- No design prerequisites remain unresolved. Implementation verification (tests, selected-dependency checks, real-agent compatibility and independent security review) is future work governed by Q25/Q27, not evidence already obtained. Final confirmation was received in Q28.

## Risks and alternatives

- Existing manual, shell, Vault, and 1Password providers may already satisfy the actual need; a new storage type adds key-lifecycle responsibilities.
- Private per-user values and shared multi-recipient secrets require different selection and authorization semantics.
- Do not assume an SSH agent exposes private key material or supports a proposed encryption construction.
- The user explicitly chose a `.psst/` directory, superseding the captured same-config-file proposal; Q4 preserves existing reference config files.
- Signature-derived encryption elevates agent signing access into durable decryption access. Agent-only access does not mean derived encryption keys never exist in psst memory.

## Agreement

Confirmed by the user in Q28. Q1–Q27 define the accepted scope, threat boundary, operational behavior, exclusions, release gates and bounded implementation discretion. Grilling is complete. Implementation and release verification require separate work; no implementation was requested or performed during this discussion.
