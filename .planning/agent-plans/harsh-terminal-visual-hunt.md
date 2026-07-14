# Harsh terminal visual defect hunt

## Scope
Adversarial WebKit Playwright probes for terminal visual/layout defects.
Update `.planning/agent-plans/web-cockpit-defect-list.md` with confirmed failures.

## Non-goals
- Fixing defects (list only unless asked)
- HIG-only tap-size noise as critical

## Delegation decision
`not delegated because hunt/review-only — parent runs probes and writes defect list`

## Checklist
- [x] Tighten geometry probes (host→keys, blank band, fill)
- [x] Run full harsh suite on mobile-webkit
- [x] Investigate canvas-tap focus (false positive — Ghostty contentEditable works)
- [x] Probe key-row visibility (Paste / Hide keyboard off-screen → C3)
- [x] Probe narrow 320 blank band → M2
- [x] Update defect list; drop bogus C3; add real C3/M2
- [x] Validation: `playwright test e2e/explore-terminal-visual.test.ts --project=mobile-webkit`

## Validation results
5 failed / 6 passed (expected — defects open):
- empty status spacer → C2
- expand/collapse spacer → C2
- landscape spacer → C2
- Paste/Hide keyboard visible → C3
- narrow blank band → M2

## Deviations
- Earlier “canvas tap focuses non-editable DIV” was wrong: scale-layer is `contentEditable` + delivers WS input.
