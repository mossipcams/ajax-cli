# Agent Routing and Delegation

Read this document only when work will be delegated.

The active agent owns investigation, engineering decisions, review, and final
verification. Delegation changes who performs a bounded task; it does not
transfer responsibility for the result.

## Same-harness work

Use the active harness's native delegation for same-harness work: Cursor to
Cursor, Codex to Codex, and Pi to Pi when native execution is available.
Do not launch a second instance of the same harness through Ajax Model Router.

Harness-specific workflows are optional local capabilities. They must not
override this repository contract or become requirements for other harnesses.

## Cross-harness work

Use the Ajax Model Router skill only when intentionally delegating to a model in
a different harness or provider subscription. Do not duplicate provider model
rankings or exact model IDs in repository documentation; the router registry is
their source of truth.

Ajax Model Router owns:

- target-harness and model validation;
- exact provider model IDs and cross-harness transport;
- timeouts, cancellation, pre-dispatch snapshots, and post-dispatch deltas;
- write-scope enforcement and structured delegate reports;
- verification artifacts, parent review bundles, and safe restoration of
  rejected delegate changes.

It does not own:

- engineering playbook or architecture decisions;
- whether the active agent implements directly;
- same-harness delegation;
- risk-based or file-type-based model selection outside its registry.

If a requested target or model is unavailable, stop and report the constraint.
Do not silently substitute another provider or model.

## Work orders

Every cross-harness delegation must state:

- target harness and requested model;
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
