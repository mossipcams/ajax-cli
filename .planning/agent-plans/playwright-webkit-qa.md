# Critical WebKit QA hunts

## Approach change

Stopped HIG/tap-size/console-noise hunting. New suite only asserts **breakage**:
wrong-task destructive confirm, dead expand hit targets, stuck chrome after
expand+keyboard, Start buried under keyboard, terminal input dead after
expand/collapse.

## Command

```bash
./node_modules/.bin/playwright test e2e/explore-webkit-critical.test.ts \
  --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit
```

**Result: 1 failed, 4 passed**

## CRITICAL defect found

See consolidated list: [web-cockpit-defect-list.md](./web-cockpit-defect-list.md) (**C1**).

### CRITICAL-1 — New Task **Start** sits below the soft-keyboard band

- **Repro:** open New task → focus title → simulate iOS keyboard band (`--app-top:50px`, `--app-height:400px`)
- **Evidence:** Start button bottom at **y=483.5**, band ends at **450**
- **Impact:** on iPhone WebKit, operator cannot tap Start while the keyboard is open (must dismiss keyboard first — easy to miss / looks broken)
- **File:** new-task sheet layout under `html.keyboard-open` (`NewTaskSheet.svelte` / fullscreen sheet CSS)
- **Listed as:** C1 in `web-cockpit-defect-list.md`

## Critical hunts that passed (no defect)

- Drop confirm does **not** leak across task A→B hash switch (one-tap Drop on B blocked)
- Expand remains real hit target after scale + landscape rotation
- Expand → keyboard-open → collapse restores bottom-nav / chrome; Back + New still work
- Esc still reaches terminal socket after expand/collapse

## Non-critical (earlier shallow hunt — not the focus)

- viewport `interactive-widget` console noise
- 28–38px control heights (HIG, not flow-breaking)
