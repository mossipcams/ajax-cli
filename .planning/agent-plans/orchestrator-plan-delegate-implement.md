# Orchestrator plans; delegates implement

## Approval

- Status: **approved** (user: “approved for the pr creation delegation and the
  orchestrator changes”).
- Implementation was paused after a partial `ajax-model-router` skill edit;
  resume under this plan. Parent plans and reviews; a delegate writes.

## Problem

The parent orchestrator (this Cursor chat model) has been writing code, opening
PRs, and sometimes skipping a written plan. Native Task fan-out and parent-local
`Write`/`gh pr create` are the same class of cost: the orchestrator doing worker
work.

User direction:

1. Ajax Model Router (the selected delegate) creates PRs, not the orchestrator.
2. The orchestrator never implements; it only delegates implementation.
3. Pause the in-flight edit. The orchestrator should also **plan**.

## Role split

| Role | Owns | Must not |
| --- | --- | --- |
| **Parent (orchestrator)** | Understand the request; write the plan when required; emit one `EXECUTION`; review the actual delta; accept/revise/discard; report the PR URL | Product/docs writes, commits, pushes, `gh pr create`, merge/rebase/force-push |
| **Router** | Executor, model, risk, scope, verify, fallback | Playbook design, parent-local writes |
| **Delegate** | Investigation inside scope, implementation, verification, and — when the user asked — commit, push, and `gh pr create` | Merge, rebase, force-push, extra worktrees, expanding `SCOPE` |

`AGENT: parent` / `R-PARENT` remains **Q&A and planning with no write**.
Architecture planning that needs a persistent plan is parent-local. The first
implementation write still goes through a delegate.

Trivial one-liners are not a parent-write exception. Skip the router only for
pure Q&A/exploration with no file changes.

A user “create PR” request is authorization for the **delegate** to commit,
push, and `gh pr create` after the repo’s local gate (Husky in ajax-cli). It is
not authorization for the parent to do those steps. It is not merge or rebase.

## Partial work already on disk (paused)

`ajax-model-router/skills/model-router/SKILL.md` already says the parent never
implements and that user-requested PRs are delegate-owned. It is **inconsistent**:

- Roles still give **planning** to the delegate.
- Dispatch prompt and `libexec/lifecycle_hooks.py` `DISPATCH_WRAPPER` still say
  never commit unless a commit was requested (no PR path; parent still implied
  as the PR actor).
- ajax-cli `AGENTS.md`, `docs/agent/routing.md`, and
  `docs/agent/pull-requests.md` still tell delegates not to commit/push and
  still allow skipping the router for trivial writes.

Do not finish those edits until this plan is approved.

## Scope

- `ajax-model-router`: `skills/model-router/SKILL.md`, matching
  `DISPATCH_WRAPPER` in `libexec/lifecycle_hooks.py`, README safety blurb if it
  still says parent may write, contract tests/`check-contracts` only if a pinned
  phrase must move.
- ajax-cli: `AGENTS.md` Delegation, `docs/agent/routing.md`,
  `docs/agent/pull-requests.md` (who runs `gh pr create`; keep the Husky gate).
- Copy the ajax-cli doc delta to open worktrees after the PR lands or as
  uncommitted overlays, same as prior agent-contract rollouts.

## Non-goals

- No lifecycle/registry/runtime behavior change.
- No force-push, merge, or rebase rights for delegates.
- No Grok (or other) model IDs outside the router registry.
- No pstack auto-run.
- Do not edit `/Users/matt/Desktop/Projects/AGENTS.md` or
  `~/.cursor/rules/composer-first-delegation.mdc` in this change (call those
  out as leftover conflicts if they still tell the parent to write).

## Implementation tasks (after approval)

- [x] Align router Roles: parent owns **planning**, routing, and review;
      delegate owns implementation and user-requested git/PR.
- [x] Align Dispatch + `DISPATCH_WRAPPER`: PR request ⇒ delegate may commit,
      push, `gh pr create`; still no merge/rebase/force-push.
- [x] ajax-cli `AGENTS.md` + `routing.md`: orchestrator plans and delegates;
      never implements; no trivial-write skip.
- [x] `pull-requests.md`: model-router delegate opens the PR; parent reviews
      and reports the URL; Husky gate unchanged.
- [x] `scripts/check-contracts` in ajax-model-router.
- [x] Parent reviews the actual diff; delegate (not parent) opens PRs if asked.

## Verification

```bash
cd /Users/matt/Desktop/Projects/ajax-model-router && bash scripts/check-contracts
```

Ajax-cli: Husky on the agent-doc commit when a PR is requested.

## Validation results

- Delegate ran `bash scripts/check-contracts`: 71 tests OK (1 skipped), exit 0.
- Parent inspected the git diffs in ajax-model-router and the restore worktree;
  they match the approved split. No PRs opened (user did not ask).

## Remaining risk

- Native Cursor `Task` is forbidden in `composer-first-delegation.mdc`,
  the router dispatch prompt, and `cursor-delegate`. acpx still has no
  `--disallowed-tools`, so a model that ignores the prompt can still fan out.
- `cursor-delegate` still uses Cursor Composer via acpx, so implementation
  tokens stay on the Cursor plan. This change only stops extra Task hops
  and the orchestrator doing the writes.
