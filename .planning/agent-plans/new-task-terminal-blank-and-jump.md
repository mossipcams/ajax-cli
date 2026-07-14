# Investigate: new-task terminal blank + delayed jump-down

## Scope
Reproduce and document two operator reports (Web Cockpit, mobile):
1. Massive empty space in the terminal after creating a task via web — CLI should sit just above the keyboard.
2. Terminal jumps downward after a few seconds of use.

Non-goals: implementing fixes in this pass (defect list + failing repros first).

## Root cause (confirmed)

`terminalLayoutPolicy.allowLocalFit` is false while `keyboard-open`.
`fitNow` then only crops the pre-keyboard canvas to the bottom.

- Fresh CLI prompt at top of grid → scrolled **off-screen** (~172px above host).
- Visible band above keys = empty lower rows → operator report of massive space.
- Keyboard dismiss restores header/interact/nav → host **jumps down ~97–105px**.

## Delegation decision
`not delegated because hunt/repro-only — parent owns probes and defect list`

## Checklist
- [x] Probe mid-session keyboard open → crop blank (C4)
- [x] Probe keyboard dismiss → host jump (C5)
- [x] Probe new-task handoff dismiss jump (C5)
- [x] Add `e2e/explore-keyboard-blank-jump.test.ts` (3 failing)
- [x] Update `web-cockpit-defect-list.md` (C4 / C5)
- [x] Validation recorded below

## Validation

```bash
playwright test e2e/explore-keyboard-blank-jump.test.ts --project=mobile-webkit
```

3 failed (expected):
- cropped empty band → C4 (`canvasAboveHost=172`)
- must not jump → C5 (`jumpDown=97`)
- new-task handoff → C5 (`jumpDown=105`)
