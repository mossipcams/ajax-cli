# Agent-sized scale-to-fit

## Scope

Phone Web PWA: logical terminal cols `max(80, hostFitCols)`, CSS scale-to-fit
host width, PTY lockstep with logical size. Live and scrollback share layout.

## Non-goals

- No wterm migration, Wide toggle, transcript/read-mode, or tmux width query (v1).
- No pinch-magnifier redesign (scale&lt;1: no fit-down via pinch).

## Delegation decision

`Delegation decision: delegated via model-router` → packet critique PASS
(codex gpt-5.5) → cursor-delegate / composer-2.5 implement.

## Task checklist

- [x] Packet READY + critique PASS
- [x] Delegate implement
- [x] Parent review + validation

## Validation results (parent)

- web:test geometry/TerminalRawView/zeroLag/selection/state/TaskList: 247 PASS
- playwright mobile-webkit terminal-scroll-garble: 4 PASS
  - softwrap diag: cols=80, fitScale≈0.54, no cra/tes phone wrap
- web:check: 0 errors
- git diff --check: PASS

## Deviations

- Delegate RED report listed EXIT_CODE 0 with failure excerpt (report hygiene);
  parent re-ran GREEN verification independently and accepted.
- `logicalRows` validates proposed fit rows (fallback 24); does not recompute
  from hostHeight/scale (acceptable for v1).
