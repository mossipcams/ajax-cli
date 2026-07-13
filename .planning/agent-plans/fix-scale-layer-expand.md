# Fix expand button / scale applied to terminal-host

## Scope

Ghostty `open(parent)` assigns `this.element = parent`. Scale-to-fit was
transforming `.terminal-host`, visually shrinking the host. Open into an inner
`.terminal-scale-layer` and scale that instead.

## Delegation decision

`Delegation decision: not delegated because this is a tight regression fix
against the in-flight scale-to-fit change; failing e2e already names the
overflow/hit contract.`

## Task checklist

- [x] RED e2e: hostScrollW ≈ hostClientW after fit; expand still works
- [x] Inner scale layer + open(scaleLayer); transform only that node
- [x] Mock `open` assigns `this.element = parent` (mirror Ghostty)
- [x] Parent verification

## Validation

```bash
rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalRawView.test.ts
# 141 passed

rtk npx playwright test e2e/fullscreen-refit.test.ts e2e/terminal-scroll-garble.test.ts \
  --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit
# 10 passed (including expand hit-target)

rtk npm run web:check
# 0 errors
```

## Deviations

None. Root cause was scale transform on `.terminal-host` via Ghostty
`element === parent`, not a broken expand handler.
