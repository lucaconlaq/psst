---
name: conventional-commit
description: Create a git commit using the Conventional Commits format. Use when the user asks to commit staged files, stage a meaningful set of changes for commit, generate a commit message, or make and optionally push a conventional commit.
---

# Conventional Commit

Create a git commit using the Conventional Commits format.

Prefer committing currently staged files only.
If nothing is staged, inspect the working tree, select a cohesive meaningful change, stage it, and prepare a proposed commit without asking for separate staging confirmation.
Include new files when they are clearly part of the selected change, but never stage unrelated changes merely because they are present.
If no coherent staging choice can be made safely, ask the user to choose between the plausible groups.
Always ask for confirmation before committing.

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

Use the staged diff when deciding what to commit if staged changes exist.
If nothing is staged, inspect unstaged changes to select and stage a cohesive commit.
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
7. Add a footer only when needed, such as `BREAKING CHANGE:` or issue references.
8. Never use `!` in the commit header, even for breaking changes. Describe compatibility changes in the body or footer when needed.

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
10. If the user confirms, create the commit.
11. Show the commit result.
12. After a successful commit, ask whether the user wants to push.
13. Push only if the user confirms.

## Commit command

For a single-line message:

```bash
git commit -m "type: description"
```

For a message with body or footer, use multiple `-m` flags:

```bash
git commit -m "type: description" -m "Body text." -m "Footer text."
```

Run `git add` only for the cohesive change selected by this workflow. Never use `git add -A` when unrelated changes are present.

After a successful commit, ask whether to push. If confirmed, use the current branch's configured upstream with:

```bash
git push
```

If there is no configured upstream, explain that and ask before using a command such as:

```bash
git push -u origin <current-branch>
```
