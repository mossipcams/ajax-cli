# Hunt: C4/C5-shaped keyboard/layout defects

## Scope
Find defects with the same shape as C4/C5 (fit freeze, crop blank, chrome jump).

## Delegation decision
`not delegated because hunt/repro-only`

## Checklist
- [x] Inspect layout policy + fit/crop call sites
- [x] Probe expand / orientation / reconnect / paste / app-top / settle
- [x] Add failing repros for confirmed siblings (`explore-c4c5-siblings.test.ts`)
- [x] Update defect list (**C6**)
- [x] Record validation

## Confirmed new
- **C6** — host shrink under keyboard (reconnect status, paste fallback, keyboard settle) deepens C4 crop

## Not defects (same-shaped probes)
- Expand/collapse jump (restores correctly)
- Expand-under-keyboard (temporary allowLocalFit clears crop)
- Address-bar offsetTop drift
- Task remount under keyboard (mild)
- Orientation (only C2 residual)

## Validation
```bash
playwright test e2e/explore-c4c5-siblings.test.ts --project=mobile-webkit
```
Expected: 3 failed (C6 variants).
