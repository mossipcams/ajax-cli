# Fix PR #749 File LOC (TaskTerminal peel)

## Scope

Peel `TaskTerminal.tsx` under the 1000-line hard File LOC limit while
preserving scrollOnErase latch and speech UX.

## Non-goals

- No behavior change to latch, seed reveal, or speech
- No File LOC script / limit changes
- No mandatory `TaskTerminal.test.tsx` split
- Do not reintroduce server pad
- Do not edit the Cursor plan file

## Delegation decision

`Delegation decision: not delegated because R-SIZE-SPLIT (mount peel alone ≈700
lines; approved CI fix is a mechanical ownership peel — parent implements)`

```yaml
ROUTING_DECISION:
  ACTION: LOCAL
  LANE: local
  MODE: NONE
  MODEL: NONE
  PACKET_STATUS: NOT_REQUIRED
  PACKET_REBUILD_COUNT: NONE
  PACKET_CRITIQUE_COUNT: NONE
  ALLOWED_SCOPE:
    - crates/ajax-web/web/src/features/task/TaskTerminal.tsx
    - crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx
    - crates/ajax-web/web/src/features/task/useTaskTerminalSpeech.ts
    - crates/ajax-web/web/src/features/task/mountTaskTerminalSession.ts
    - .planning/agent-plans/task-terminal-file-loc-peel.md
  REASON: Mechanical LOC peel; mount body exceeds pre-dispatch size-split.
  ESCALATE_IF: [behavior change, TaskTerminal still >= 1000]
```

## Checklist

- [x] Task 1 — Extract `useTaskTerminalSpeech.ts`
- [x] Task 2 — Extract `mountTaskTerminalSession.ts` (preserve latch sites)
- [x] Task 3 — Update source-assert tests if latch strings moved
- [x] Task 4 — Verify LOC + web tests/check/build; push; confirm File LOC

## Approval status

User approved attached plan “Fix PR #749 File LOC failure”.

## Deviations

- Parent implemented (not delegated): mount peel alone exceeds R-SIZE-SPLIT.
- Source-assert tests now concatenate shell + mount + speech raw sources;
  latch/seed asserts read `mountTaskTerminalSession.ts` directly.
- Mount cleanup calls `cancelSpeechTransport` via `useEffectEvent` so the
  `[handle]` mount effect stays exhaustive-deps clean.

## Validation

```bash
wc -l …/TaskTerminal.tsx          # 984 (< 1000)
FILE_LOC_BASE=… FILE_LOC_HEAD=HEAD node scripts/check-file-loc.mjs
  # 0 errors; warnings only for TaskTerminal.tsx (984) and test (676)
npm run web:test -- --run TaskTerminal.test.tsx scrollbackOverwriteProbe.test.ts  # 35 pass
npm run web:check   # pass
npm run web:lint    # pass
npm run web:build   # pass
```
