# Packet — Mic second-tap finalize

```yaml
PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior
dispatch_level: compact
```

## Task

Make Mic a complete start/stop control: while `listening` or `pause_pending`,
tapping Mic enters `finalizing` and calls the existing speech transport
`stop()` path (same finalization used when spoken pause grace expires). Keep
already-inserted PTY text. Cancel voice stays abandon-only.

## Scope

### Allowed

- `architecture.md` (Speech Input Architecture Mic / completion wording only)
- `docs/speech-input.md` (normal use / Mic stop wording only)
- `crates/ajax-web/web/src/shared/lib/speechState.ts`
- `crates/ajax-web/web/src/shared/lib/speechState.test.ts`
- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`
- `.planning/agent-plans/mic-stop-finalize.md` (checklist only)

### Forbidden

- Backend/Rust STT route, provider, or protocol changes
- PTY / tmux / Enter auto-submit changes
- Changing Cancel voice abandon semantics
- Renaming visible Mic label away from `Mic`
- Unrelated files, commits, pushes, branch changes

## Acceptance

1. From `listening` or `pause_pending`, Mic click dispatches a reducer action that
   transitions to `finalizing` and then calls `speechTransportRef.current?.stop()`.
2. From `idle` / `error`, Mic still starts a fresh session (existing behavior).
3. Mic remains disabled in `connecting` / `finalizing`.
4. Visible button text stays `Mic`. When stoppable (`listening` /
   `pause_pending`), `aria-label` and `title` are `Stop voice input`; otherwise
   `Start voice input`.
5. Spoken `pause` / `start over` and Cancel voice paths still work.
6. Focused speechState + TaskTerminal tests cover the new stop path; source
   contracts that assumed Mic-only-start are updated.

## Constraints

- Reuse existing `transport.stop()` / `finalization_complete` / `onClosed`
  finalizing path (mirror pause-elapsed stop wiring around TaskTerminal ~806–808).
- Add one pure reducer action (suggested name `request_stop`) with sessionId
  guard; only accept from `listening` or `pause_pending`.
- Clear pause countdown UI when entering finalizing via Mic stop.
- Keep diffs small; no new abstractions.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: rtk npm run web:test -- --run crates/ajax-web/web/src/shared/lib/speechState.test.ts
      expected: pass, including request_stop coverage
    - type: test
      command: rtk npm run web:test -- --run crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx
      expected: pass, including Mic stop aria/wiring coverage
    - type: typecheck
      command: rtk npm run web:check
      expected: pass
  broader_checks: []
  reason: Pure reducer + TaskTerminal wiring; focused Vitest + typecheck prove behavior.
```

## Stop if

- Need backend protocol or provider changes
- Diff grows past allowed files or ~250 lines of unrelated churn
- Verification fails and root cause is outside Mic stop wiring
- Architecture conflict: cannot keep visible `Mic` label while adding stop

## Code anchors

- `TaskTerminal.tsx` `activateMic` (~691): currently no-ops unless idle/error
- `TaskTerminal.tsx` Mic button (~1881–1896): always Start aria; calls only `activateMic`
- `TaskTerminal.tsx` pause elapsed (~806–808): existing `speechTransportRef.current?.stop()`
- `speechState.ts`: no stop-from-listening action yet; `pause_elapsed` → finalizing
- `speechTransport.ts` `stop()` (~523): sends `stt.stop`, releases capture

## Edit instructions

1. Docs: state that a second Mic tap finalizes (keep Cancel as abandon).
2. `speechState`: add `request_stop` action + tests.
3. `TaskTerminal`: `toggleMic` / split start vs stop; stop path dispatch +
   `transport.stop()`; update aria/title; update tests.
4. Do not commit.
