---
name: push
description: Push code when requested, asking for confirmation if any outgoing commits are unsigned.
---

# Push

1. Use the requested destination or the current branch's upstream. Ask only if the destination is unclear.
2. Fetch the remote, then list outgoing commits with `git rev-list HEAD --not --remotes=<remote>` (or use the requested source instead of `HEAD`). Stop if fetching or inspection fails.
3. Use Git directly: run `git cat-file commit <id>` for each outgoing commit and inspect its output. Do not use Python or other scripting runtimes for this check. A signed commit has a `gpgsig ` or `gpgsig-sha256 ` header before the first blank line. Check these actual headers, never the commit message. Missing verification keys do not make a commit unsigned.
4. If any are unsigned, list their IDs and subjects and use `ask_user_question` to ask "Push these unsigned commits nonetheless?" with `Cancel` and `Push anyway` options. Stop unless the user approves.
5. Push normally to the intended destination and report the result. Do not create commits or force-push unless explicitly requested.
