---
name: grill-idea
description: Grill or stress-test the user's thinking about an existing idea in docs/ideas/, including any grill trigger phrases. Requires an idea document already there; use add-idea first if none exists.
---

Read [documentation-model](../documentation-model/SKILL.md) and [IDEA-FORMAT.md](../documentation-model/IDEA-FORMAT.md). Read the supplied or clearly matching existing idea in `docs/ideas/`; ask the user to choose if ambiguous. If none exists, stop and direct the user to `add-idea`. Never create an idea through this skill.

Announce the idea path. Before reopening, check for its same-filename spec in `docs/specs/` and tickets linked to that spec or idea. If any exist, list their exact paths and use `ask_user_question` to confirm deleting them before proceeding. If their association is unclear, ask rather than guessing. Delete only the confirmed files; if confirmation is declined or cancelled, stop without changing any files.

Then set `Status: Brainstorming`. Preserve the previous agreement as historical, not current approval. Resume existing decisions and open questions rather than starting over.

Interview the user relentlessly until you reach a shared understanding. Map a **design tree**: every decision branches into the decisions that depend on it.

Work in **rounds**. The **frontier** is every decision whose prerequisites are settled. Cover the whole frontier before advancing to dependent decisions. Never ask a question whose answer depends on another unanswered question in the same round. Recompute the frontier after each answered round.

Use `ask_user_question` from `@juicesharp/rpiv-ask-user-question` for every grilling round and final confirmation—not plain-text question lists. Ask up to four independent frontier questions per call, with stable numbered IDs and 2–4 choices each. If more remain, ask them in subsequent batches before advancing to dependent questions.

Give each question a clear title and body. Put your recommended answer first, labeled “(Recommended)”, and word the question so “yes” accepts it. Use option previews when concrete alternatives benefit from side-by-side comparison.

Finding **facts** is your job; **decisions** are the user's. Dispatch a sub-agent to investigate environmental facts if available; otherwise use tools yourself. Never ask for facts you can look up. A running investigation is an unsettled prerequisite: defer only its dependent questions and ask the rest of the frontier without waiting when tools allow.

Update the idea after each answered round using the shared format. Preserve existing notes; record decisions and rationale, never unanswered recommendations as agreement. Save unresolved questions and investigations when paused.

Finish only when the frontier is empty: every branch is visited and settled or explicitly excluded, with no unresolved prerequisites or silently assumed decisions. Summarize the shared understanding and ask the user to confirm it with `ask_user_question`. Only after confirmation, set `Status: Agreed` and record the agreement; do not act on the plan before confirmation.

Modify only the existing idea document, except for the confirmed deletions above and glossary updates allowed by `documentation-model`. Implementation requires a separate request.
