# Agent Routing and Delegation

Read this document only when work will be delegated.

The active agent owns investigation, engineering decisions, review, and final
verification. Delegation changes who performs a bounded task; it does not
transfer responsibility for the result.

## Model routing

Always use the `model-router` skill for implementation writes. Do not spawn
native harness subagents (Cursor Task, best-of-n, Claude/Codex task children,
or pstack explorers) instead of the router. It emits one `EXECUTION` decision
(agent, model, risk, scope, verify, fallback), then execute and verify. Skip
it only for pure Q&A or exploration with no file changes, or when the user
explicitly approved a parent-local bypass for this request. Without that
approval the orchestrator plans and does not implement.

Do not duplicate provider model rankings or exact model IDs in repository
documentation; the router registry is their source of truth.

Ajax Model Router owns:

- executor, model, risk, scope, verification expectation, and fallback;
- exact provider model IDs;
- timeouts, cancellation, pre-dispatch snapshots, and post-dispatch deltas;
- write-scope enforcement and structured delegate reports;
- verification artifacts, parent review bundles, and safe restoration of
  rejected delegate changes.

It does not own engineering playbook or architecture decisions.

Delegate dispatch transport is acpx ACP (one client for cursor, codex, and
pi); install `acpx` and keep it on `PATH`. Do not substitute native Cursor
Task or other same-harness subagents for acpx dispatch. A Cursor acpx
session must not spawn Task children.

If a requested target or model is unavailable, stop and report the constraint.
Do not silently substitute another provider or model.

Harness-specific workflows are optional local capabilities. They must not
override this repository contract or become requirements for other harnesses.

## Work orders

Every delegated task must state:

- one bounded task;
- allowed files or write scope;
- observable acceptance criteria;
- relevant verification;
- explicit stop conditions.

Do not pre-explore the repository to perfect scope or gather implementation
context. State outcome, acceptance criteria, and a bounded `SCOPE` on
`EXECUTION`; the delegate investigates inside that scope. If `SCOPE` is wrong,
the delegate stops and you emit a new `EXECUTION` — do not explore to fix scope
first. Do not delegate a vague request without outcome and acceptance.

## Acceptance and review

Before accepting delegated work, the active agent must:

1. Inspect the actual delta.
2. Confirm it stayed within the allowed scope.
3. Check it against the acceptance criteria.
4. Confirm the reported verification was relevant and passed.
5. Run additional focused validation when needed.
6. Reject or safely restore unrelated, incomplete, or unsupported changes.

An empty diff with a success claim is a failure. A delegate report is evidence,
not approval.

When the user asks to create a PR, the selected delegate runs the repository's
local verification gate, commits, pushes, and runs `scripts/gh-pr-create`; the
orchestrator reports the PR URL after reviewing the delta. Delegates must not
merge, rebase, force-push, or switch branches unless the user explicitly
authorizes that behavior. A commit or pull-request request implies commit,
push, and `scripts/gh-pr-create`.
