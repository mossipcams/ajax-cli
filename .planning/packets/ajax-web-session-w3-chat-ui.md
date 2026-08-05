PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Wire Ajax Web Session mobile chat UI to the Wave 2 `/web-session` WebSocket: scrollable history, composer, send/stop, streaming assistant text, and clear running/waiting status.

## Scope

### Allowed
- crates/ajax-web/web/src/features/session/AjaxWebSessionView.tsx
- crates/ajax-web/web/src/features/session/AjaxWebSessionView.test.tsx
- crates/ajax-web/web/src/features/session/webSessionTransport.ts
- crates/ajax-web/web/src/features/session/webSessionTransport.test.ts
- crates/ajax-web/web/src/features/session/types.ts (optional)
- crates/ajax-web/web/src/styles.css
- crates/ajax-web/web/src/shared/lib/api.ts (only if a small helper for WS URL is required; prefer local to session feature)
- .planning/agent-plans/ajax-web-session-poc.md

### Forbidden
- Symbol search / chips / detail sheets (Waves 4–5)
- Terminal integration
- Changing Settings flag or TaskDetail Cursor gate behavior
- Rust backend changes (Wave 2 is done)
- Commits / branch changes
- Enabling session for non-Cursor agents

## Acceptance

1. `AjaxWebSessionView` shows: scrollable message list, large mobile composer textarea, Send button when idle/waiting, Stop button when running, visible status chip for running|waiting|error.
2. On mount, open authenticated same-origin WebSocket to `/api/tasks/{encodeURIComponent(handle)}/web-session` (cookie auth like terminal/STT; reuse origin patterns from `terminalConnection` / `speechTransport`).
3. Client sends `{type:"session.prompt", version:1, message}` and `{type:"session.abort", version:1}` matching Rust wire types.
4. Handle server events: ready, status, assistant_delta (append/stream into current assistant bubble), settled, error, closed. User messages appear immediately on send.
5. Mobile-first CSS: large touch targets, readable bubbles, minimal chrome, composer stays usable on phone; no multi-panel IDE layout.
6. Focused vitest with mocked WebSocket covering connect→prompt→delta→settled and abort path.
7. Wave 3 checklist marked done in the plan.

## Constraints

- Keep files near ~600 LOC; split transport vs view if needed.
- Do not invent a second task model; messages are ephemeral UI state for the POC.
- Prefer existing Ajax button/styles patterns.
- When WS fails, show an error state in the session view (do not fall back to terminal).

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run src/features/session
      expected: pass
  broader_checks: []
  reason: Session UI + transport are frontend-only; vitest with mocked WS is sufficient.
```

## Stop if

- Need Rust changes
- Need symbol search
- Edits outside Allowed
- Patch exceeds ~400 changed lines

## Code anchors

- Placeholder view: `crates/ajax-web/web/src/features/session/AjaxWebSessionView.tsx`
- Terminal WS URL pattern: `crates/ajax-web/web/src/shared/lib/terminalConnection.ts`
- STT transport patterns: `crates/ajax-web/web/src/shared/lib/speechTransport.ts`
- Wire types: `crates/ajax-web/src/slices/web_session.rs` (`session.prompt`, `session.abort`, `session.assistant_delta`, `session.status`, …)

## Edit instructions

1. Add `webSessionTransport.ts` owning WS lifecycle and event callbacks.
2. Expand `AjaxWebSessionView` into chat UI consuming the transport.
3. Add mobile CSS under `.ajax-web-session*`.
4. Tests with fake WebSocket.
5. Check off Wave 3 in the plan.
