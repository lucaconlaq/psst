---
name: to-tickets
description: Break an agreed spec, plan, or conversation into tracer-bullet tickets with explicit blockers. Use when asked to turn agreed work into tickets; create local ticket files only after approval.
disable-model-invocation: true
---

# To Tickets

Read [documentation-model](../documentation-model/SKILL.md) and [TICKET-FORMAT.md](../documentation-model/TICKET-FORMAT.md) for ticket formats, naming, and content rules.

Create demoable or independently verifiable vertical slices, each declaring the tickets that block it. Always write local files under `docs/tickets/<feature-slug>/`.

## 1. Gather context

Read the supplied spec or plan in full, or use the source clearly identified in the conversation; ask if ambiguous. Announce the source.

For this repository, prefer the matching document in `docs/specs/` and read its linked idea in `docs/ideas/`. Follow [the spec format](../documentation-model/SPEC-FORMAT.md) and [the idea format](../documentation-model/IDEA-FORMAT.md) when interpreting decisions and readiness. Require an agreed scope with no unresolved design prerequisites. If only an idea exists, direct the user to [grill-idea](../grill-idea/SKILL.md) if not agreed, then [to-spec](../to-spec/SKILL.md) before ticketing it. For a standalone plan or conversation, require equally clear, user-confirmed scope.

Preserve settled contracts, exclusions, rationale, risks, testing obligations, release gates, and bounded implementation discretion. Do not promote historical proposals or proposed testing seams into agreed decisions. If source documents conflict, stop and report the conflict rather than choosing a new design.

## 2. Explore

Inspect relevant current code, tests, and project documentation; reuse verified context where still current. Use the project's existing terminology and follow repositories guidelines. Identify small behavior-preserving prefactors that make the change easier; schedule necessary prefactors first, with regression coverage.

## 3. Draft slices

- Each ticket completes a narrow path through all relevant layers, including tests—not a schema-only, API-only, or UI-only task.
- Each completed slice is demoable or independently verifiable and fits one fresh context window.
- Include success, failure, compatibility, and safety criteria relevant to that slice. Account for all source requirements and release gates without duplicating them across tickets.
- Inspect any existing ticket set before assigning numbers or dependencies. Continue after its highest number without renumbering existing tickets; do not duplicate work already covered by existing tickets.
- Declare only genuine blocking edges; keep the graph acyclic. No outstanding blockers means a ticket is executable, not authorized for implementation. Do not serialize independent work for convenience.

**Wide mechanical refactors are the exception:** use expand–contract. Add the new form beside the old, migrate callers in bounded batches blocked by expansion, then remove the old form in a ticket blocked by every migration. Keep CI green between batches. If batches cannot land green independently, use a shared integration branch and make all batches block a final integrate-and-verify ticket; promise green only there.

## 4. Get approval

Present a numbered list with each ticket's **Title**, **Blocked by**, and **What it delivers**. Use `ask_user_question` to ask whether granularity and genuine blocking edges are right, and whether any tickets need merging or splitting. Iterate until the user explicitly approves. Do not create ticket files before approval.

## 5. Write local tickets

Create one file per ticket under `docs/tickets/<feature-slug>/` using `TICKET-FORMAT.md`, in dependency order, blockers first. Set initial status to `blocked` if any listed blocker is not `done`, otherwise `ready-for-agent`. Follow the format's numbering and lifecycle rules when extending existing ticket sets. Never overwrite existing tickets without an explicit update request.

Report the created paths and the initial frontier: newly created tickets marked `ready-for-agent`, with no outstanding blockers. Stop—ticket creation does not authorize implementation. Except for glossary updates allowed by `documentation-model`, modify only the approved ticket files. Leave source ideas, their statuses, specs, and project code unchanged.
