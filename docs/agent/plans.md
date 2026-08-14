# Agent Plans

Read this document when the user requests a plan, the work has multiple
dependent implementation steps, the task needs a durable handoff, or the change
affects architecture or security.

## Planning-only work

For a plan, review, critique, investigation, or design request, inspect the
relevant source and tests and provide an evidence-backed result. Do not edit
code unless implementation was also requested.

## Architecture and security changes

Before changing ownership, dependency boundaries, task truth, registry or
lifecycle semantics, terminal behavior, runtime authority, public contracts,
or security assumptions:

1. Read root `architecture.md` and the focused subsystem document it links.
2. Create a written plan.
3. Wait for approval unless the user explicitly requested immediate
   implementation.
4. Update the owning architecture documentation in the same change.

## Persistent plans

Create `.planning/agent-plans/<short-slug>.md` when:

- the user requests a persistent plan;
- work spans multiple dependent implementation steps;
- architecture or security is affected;
- a durable handoff across sessions or agents is needed.

Do not create one for a trivial, localized, or mechanical change merely to
satisfy process.

Keep an active plan current. It must include:

- scope and non-goals;
- implementation and verification tasks with current checklist status;
- approval status;
- material deviations or changed assumptions;
- validation commands and results.

The final report must name the plan path, state whether its checklist is
complete, and report any remaining work or risk.
