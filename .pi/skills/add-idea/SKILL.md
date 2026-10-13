---
name: add-idea
description: Add a new idea to docs/ideas/ without grilling or implementing it. Use when the user asks to save, jot down, or park an idea.
---

Read [documentation-model](../documentation-model/SKILL.md) and [IDEA-FORMAT.md](../documentation-model/IDEA-FORMAT.md) for the idea document format and naming rules.

Capture the user's idea faithfully in a new document. Preserve uncertainty; don't invent requirements or decisions. Ask only if the subject is unclear, not for missing design details.

If a matching idea already exists, ask whether to create a separate idea or keep the existing one; never overwrite or update it through this skill.

Create only the new idea document with `Status: Parked`. Report its path and stop—no feasibility investigation, grilling, or implementation. Grilling is a separate step: use [grill-idea](../grill-idea/SKILL.md) on the existing document when the user requests it.
