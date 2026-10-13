---
name: review-rules
description: Check code for compliance with project rules. Use for rule, guideline, or style compliance reviews. Default to edited files and report only; confirm any user-defined file selection and an explicit request to fix findings immediately.
---

Default to files with staged, unstaged, or non-ignored untracked changes without asking for confirmation. If there are no edits in the default scope, say so and stop. Accept any user-defined selection, not just a file, folder, whole project, or commit. For a non-default selection, resolve it to a concrete scope and use `ask_user_question` to confirm that scope before reviewing; stop if confirmation is declined or cancelled. 

First, inventory the selected scope and print a checklist of every file to review, including deleted paths. Identify excluded generated outputs, caches, and vendored code separately. Keep the checklist updated as each file is reviewed; review every listed file fully, never silently skip one, and do not claim completion while any remain unchecked. If blocked or interrupted, report checked/total counts and the remaining paths.

Read applicable `AGENTS.md` instructions, `docs/rules/index.md`, and the rule documents matching the selected files. Repository paths are relative to the repository root, not this skill directory. If required rules are missing, report the blocker rather than claiming compliance.

Announce the scope and review each selected file against applicable rules. Read full files and relevant diffs; distinguish staged and unstaged changes, and account for additions, deletions, and renames. Follow dependencies only as supporting context. For edits or a commit, prioritize introduced violations and label pre-existing ones. Disclose exclusions and incomplete coverage.

Report actionable findings by severity with `path:line`, the violated rule and section, and a concise proposed correction. Do not present personal preferences as rules. Summarize checks performed and uncertainties; if none were found, say "No rule violations found in the reviewed scope."

Review only by default: do not modify files or Git staging. Run relevant non-mutating checks with the documented toolchain when useful. If the user explicitly requests immediate fixes, apply confirmed corrections without another confirmation, preserve unrelated edits, and rerun relevant checks. For a historical commit, do not check it out or overwrite current work; ask if the fix target is unclear. Report fixes and remaining findings; never commit or push unless separately requested.
