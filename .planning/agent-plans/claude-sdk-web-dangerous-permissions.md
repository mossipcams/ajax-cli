# Plan: Claude Agent SDK (web) runs with dangerous permissions

Status: complete. Approved for immediate implementation by explicit user
request ("We should be running the Claude sdk for web using dangerous
permissions"). No stop condition triggered: this changes only the Claude SDK
web session permission mode; authentication, network exposure, task lifecycle,
and other harness paths are untouched.

## Scope

- `crates/ajax-web/sidecar/claude_sdk_sidecar.mjs` — init options for the
  Claude Agent SDK query.
- `docs/architecture/web-session-behavior.md` — owning contract for the web
  orchestration session (permission behavior).
- This plan file.

## Non-goals

- No changes to ACP harnesses (Cursor, Codex) or Pi RPC: they keep their
  existing config-mode selection and host auto-approve behavior.
- No changes to the CLI `__agent-runtime` Claude path
  (`--dangerously-skip-permissions` is already set there).
- No test files are modified; no assertions are weakened.

## Change

The sidecar currently relies only on a `canUseTool` callback that always
allows, with no permission-mode flag (deliberate at the time — see
`.planning/agent-plans/claude-agent-sdk-pi-rpc.md`). The Claude Agent SDK
(installed `@anthropic-ai/claude-agent-sdk`, `sdk.d.ts`) exposes
`permissionMode: 'bypassPermissions'` which *requires*
`allowDangerouslySkipPermissions: true`.

In `handleInit`, add both to the `query()` options so the Claude SDK web path
runs with dangerous (bypassed) permissions. `canUseTool` stays as a defensive
fallback; in bypass mode the SDK does not surface permission prompts for it.

## Checklist

- [x] Inspected sidecar, client, spawn, tests, and SDK type definitions.
- [x] Read root `architecture.md` (navigation/ownership sections) and
      `docs/architecture/web-session-behavior.md` (permission sections) —
      owning doc identified.
- [x] Plan created; approval status recorded (explicit user request).
- [x] Sidecar init options set `permissionMode: "bypassPermissions"` +
      `allowDangerouslySkipPermissions: true`; header comment updated.
- [x] `docs/architecture/web-session-behavior.md` updated to reflect that the
      Claude SDK sidecar runs in dangerous/bypass permission mode while ACP and
      Pi paths keep host auto-approve.
- [x] Verification run (recorded below).

## Verification

| Command | Result |
| --- | --- |
| `node --check crates/ajax-web/sidecar/claude_sdk_sidecar.mjs` | pass (SYNTAX_OK) |
| `cargo test -p ajax-web claude_sidecar` | pass — 21 passed, 0 failed (all sidecar resolution/resume/override suites plus core sidecar tests, incl. `tool_prompt_observes_can_use_tool_allow`) |
| `cargo fmt --check` | pass (no diffs) |
| `npm run verify:arch` | pass (ajax-web + ajax-supervisor architecture rule suites green) |
