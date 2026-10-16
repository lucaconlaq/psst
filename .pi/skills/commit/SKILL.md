---
name: commit
description: Create a git commit using the Conventional Commits format. Use when the user asks to commit changes, optionally pushing only when explicitly requested.
---

# Commit

Use this skill only when the user wants to create a commit, not merely draft a commit message. Follow the workflow below using Conventional Commits.

## Context to inspect

Inspect the repository state with:

```bash
git status --short
git diff --stat
git diff
git diff --cached --stat
git diff --cached
git log --oneline -10
```

Use recent commits only as style reference.

## Conventional Commits format

The commit message must follow:

```text
<type>[optional scope]: <description>

[optional body]

[optional footer]
```

Allowed types:

- `feat`: a new feature
- `fix`: a bug fix
- `docs`: documentation-only changes
- `style`: changes that do not affect code meaning, such as formatting
- `refactor`: code changes that neither fix a bug nor add a feature
- `perf`: performance improvements
- `test`: adding or correcting tests
- `chore`: build process, tooling, dependencies, or auxiliary changes

## Message rules

1. Type is required and must be lowercase.
2. Scope is optional.
3. Include a scope only if the user explicitly requests one.
4. Do not infer or invent scopes automatically.
5. Description must be concise and imperative, for example `add`, not `added`.
6. Add a body only when useful for explaining context, motivation, or non-obvious changes.
7. Never use `!` in the commit header, even for breaking changes. Describe compatibility changes in the body or footer when needed.
8. If signing is unavailable, prefix the entire header with `unsigned:` without an extra space, for example `unsigned:chore: update tooling`. This is the only exception to the Conventional Commits header format.

## Workflow

1. Check whether staged changes exist.
2. If there are staged changes, analyze `git diff --cached` and continue with those staged changes only.
3. If there are no staged changes:
   - inspect unstaged changes with `git status --short`, `git diff --stat`, and `git diff`
   - if there are no changes at all, tell the user and stop
   - identify the smallest cohesive change that forms a useful commit
   - stage that change, including clearly related new files, without asking for separate staging confirmation
   - leave unrelated changes unstaged
   - ask the user only when multiple plausible groups exist and choosing between them requires user intent
4. Re-check staged changes after staging.
5. If the user provided guidance, use it while still following Conventional Commits.
6. Generate a proposed commit message.
7. Show the user:
   - the proposed commit message
   - the staged files that will be committed
8. Ask the user to confirm before committing.
9. If the user rejects the message, revise it or stop based on their feedback.
10. If the user confirms, check signing availability as described below, then create the commit. If signing is unavailable, times out, or fails, commit unsigned as described below without requiring another confirmation. Do not retry unsigned for unrelated failures.
11. Show the commit result.
12. Push only when the user explicitly requests it, following the `push` skill in `../push/SKILL.md`, including its unsigned-commit checks and confirmation. Do not prompt to push after committing.

## Signing preflight

After confirmation and before committing:

1. Inspect the effective Git signing configuration: `commit.gpgsign`, `gpg.format` (defaults to `openpgp`), `user.signingkey`, and the signing program for that format. Honor configured custom programs.
2. If signing is disabled, skip the probe and use the unsigned command below, including the `unsigned:` prefix.
3. If signing is enabled, check that the signing program and configured key are available. For SSH agent-backed keys, use a bounded `ssh-add -l` check to see whether the agent has the key. An agent check alone does not prove signing works; file-backed keys may not need an agent.
4. Run a test signature of disposable data with the configured format, program, and key, with a **5-second timeout** and forced termination after a further second. Clean up temporary files. Do not create a commit, change refs, or modify persistent Git configuration during the probe. Configuration checks alone are not sufficient; missing verification keys do not mean signing is unavailable.
5. If the probe fails or times out, report that signing is unavailable and use the unsigned command below. If the probe cannot be performed reliably, stop and explain rather than starting an unbounded signing attempt.

Also bound the actual signed commit attempt to **5 seconds**, with forced termination after a further second (for example, `timeout --kill-after=1s 5s git commit ...`). Before retrying after a timeout, ensure the process has exited and inspect `HEAD` and the index to determine whether the commit was already created. Do not create a duplicate commit or remove a lock belonging to a running process. A whole-command timeout does not prove signing caused it: retry unsigned only if the failure is attributable to signing; otherwise stop and report it.

## Commit command

For a single-line message:

```bash
git commit -m "type: description"
```

For a message with body or footer, use multiple `-m` flags:

```bash
git commit -m "type: description" -m "Body text." -m "Footer text."
```

If the signing preflight succeeds, try the normal commit command with the time limit above, preserving the repository's signing configuration. If signing is unavailable or fails (for example, a missing key, signing agent, or signing program), use signing disabled for that command only and prepend `unsigned:` to the header:

```bash
git -c commit.gpgsign=false commit --no-gpg-sign -m "unsigned:type: description"
```

Preserve any body and footer using the same additional `-m` flags. Add the prefix only once. Do not change persistent Git configuration. Report that the commit was created unsigned.

Run `git add` only for the cohesive change selected by this workflow. Never use `git add -A` when unrelated changes are present.
