# Mic stop / finalize (feature-complete speech control)

## Scope

Make the Web Cockpit Mic control a complete start/stop dictation control:
second tap while listening (or pause-pending) finalizes the session and
releases the microphone, keeping already-inserted terminal text.

## Non-goals

- No change to PTY, tmux, Enter auto-submit, or spoken `pause` / `start over`.
- No change to Cancel voice abandon semantics.
- No cloud STT, service worker, or architecture ownership moves.
- Do not rename the visible toolbar label away from `Mic`.

## Delegation decision

Delegation decision: delegated via model-router

## Desired behavior

| State | Mic tap |
| --- | --- |
| `idle` / `error` | Start a new session (existing) |
| `listening` / `pause_pending` | Enter `finalizing`, call transport `stop()` (new) |
| `connecting` / `finalizing` | Disabled / no-op (existing) |

Accessibility: keep visible text `Mic`; when stoppable, `aria-label` / `title`
become `Stop voice input`. Cancel voice remains the abandon path.

## Task checklist

- [x] Task 1 — architecture + operator docs: document Mic second-tap finalize
- [x] Task 2 — `speechState`: add `request_stop` (listening|pause_pending → finalizing)
- [x] Task 3 — TaskTerminal Mic wiring + focused tests
- [x] Task 4 — parent validation (`web:test` focused + `web:check`)

## Approval

User asked for feature-complete Mic/text dictation (start-only is insufficient).
Architecture Mic behavior update is in-scope for the same change.

## Deviations

- Delegate `report.yaml` failed schema extraction (`INVALID_STRUCTURED_REPORT`)
  even though the raw log contained a valid COMPLETE report and the patch landed.
  Parent re-ran verification and accepted on the diff + local results.
- Review-bundle `scope_violations` listed snapshot object paths under
  `.planning/router-runs/.../snap/` (transaction artifacts), not product edits.

## Validation

```bash
rtk npm run web:test -- --run crates/ajax-web/web/src/shared/lib/speechState.test.ts crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx
# 47 passed
rtk npm run web:check
# pass
```

Review Gate: ACCEPT
