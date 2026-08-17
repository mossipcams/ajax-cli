# Agent Routing and Delegation

Read this document only when work will be delegated.

The active agent owns investigation, engineering decisions, review, and final
verification. Delegation changes who performs a bounded task; it does not
transfer responsibility for the result.

## Model routing

Call the `model-router` skill first whenever a task is one bounded code
behavior change you would otherwise implement yourself. It emits one
`EXECUTION` decision (agent, model, risk, scope, verify, fallback), then
execute and verify. Skip it for trivial one-liners, non-code work, or pure
Q&A/exploration.

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

Gather enough source and test context to make the order concrete before
dispatch. Do not delegate a vague request.

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

Delegates must not commit, push, merge, rebase, create branches, or switch
branches unless the user explicitly authorizes that behavior.
