# TDD Packet: R1 — expand flush survives settle window

## 1. Goal

When entering fullscreen (⛶) while the iOS keyboard is open (or about to open),
keep `expandFlushPending` true through the entire `snapExpandedView` settle
window (~260ms of post-layout refits) so the terminal grid and PTY resize to
the full-bleed panel width. Clear the flag only after that settle completes.
This removes the blank column under the ⛶ button.

## 2. Allowed files

**Tests**

- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`

**Production**

- `crates/ajax-web/web/src/components/TerminalRawView.svelte`

**Build**

- `crates/ajax-web/web/dist/*` only via `npm run web:build`

## 3. Forbidden changes

- Do not change pinchFlushPending behavior or ordinary keyboard-open resize
  withholding outside the expand path.
- Do not edit `styles.css`, `AppViewport.svelte`, `viewport.ts`, gestures,
  paste/copy, zero-lag overlay, or Rust.
- Do not bump ghostty-web.
- No drive-by refactors or formatting sweeps.
- Do not implement R2 (textarea paint) or R3 (backspace repeat) in this packet.

## 4. Architecture context

`TerminalRawView` owns follow-output / PTY-lockstep. While `keyboard-open` is
set, `fitNow` and `sendResize` early-return unless `pinchFlushPending` or
`expandFlushPending`. Expand focuses the textarea (keyboard pops) and
`snapExpandedView` schedules refits across two rAFs plus a 260ms timeout.
Today `beginExpandFlush` clears the flag after two rAFs — before the 260ms
settle — so later settle refits freeze and leave the pre-expand column count.

## 5. Code anchors

```651:659:crates/ajax-web/web/src/components/TerminalRawView.svelte
    beginExpandFlush = () => {
      expandFlushPending = true;
      schedulePostLayoutRefit();
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          expandFlushPending = false;
        });
      });
    };
```

```684:709:crates/ajax-web/web/src/components/TerminalRawView.svelte
    snapExpandedView = () => {
      cancelExpandedSnap();
      snapVisibleTerminal();
      // ... rAF + rAF + setTimeout(260) each calling schedulePostLayoutRefit()
    };
```

```991:1001:crates/ajax-web/web/src/components/TerminalRawView.svelte
    onclick={() => {
      const next = !expanded;
      setExpanded(next);
      if (next) {
        focusTerm();
        beginExpandFlush();
        snapExpandedView();
      } else {
        blurTerm();
        refitAfterLayout();
      }
    }}
```

Existing test to mirror:
`"resizes the grid on expand even while the keyboard is open"` (~1402).
Helpers: `mountOpenTerminal`, `settleFrames`, `resizeFramesOf`, `resize` mock,
`proposedDimensions`, `setKeyboardOpen` / `document.documentElement.classList`.

## 6. Test-first instructions

Add test named:
`"keeps expand flush through the settle window while the keyboard is open"`.

Body (fake timers):

1. `proposedDimensions = { cols: 55, rows: 30 }`, `mountOpenTerminal()`,
   advance ~400ms to settle open-path refits, clear `socket.send` + `resize`.
2. `document.documentElement.classList.add("keyboard-open")`.
3. Click Expand; set `proposedDimensions = { cols: 55, rows: 60 }`.
4. Advance timers by ~50ms (early frames) — may or may not have resized yet.
5. Clear `socket.send` + `resize` again.
6. Change `proposedDimensions = { cols: 80, rows: 90 }` to simulate the
   full-bleed width that only becomes measurable after the settle layout.
7. Advance timers by 300ms (past the 260ms settle timeout + post-layout frames).
8. Assert `resize` was called with `(80, 90)` AND
   `resizeFramesOf(socket!)` contains `{ type: "resize", cols: 80, rows: 90 }`.
9. Remove `keyboard-open` class; restore real timers.

This MUST FAIL before the production edit: after ~50ms the flag is already
false, so the 260ms settle refit is swallowed by the keyboard-open guard.

Focused failing command:
```
npm run web:test -- --run src/components/TerminalRawView.test.ts -t "settle window while the keyboard is open"
```

Keep the existing early-path test
`"resizes the grid on expand even while the keyboard is open"` green.

## 7. Production edit instructions

Replace the double-rAF clear in `beginExpandFlush` so the flag stays true for
the full snap settle duration, then clears after one more post-layout pass:

```ts
beginExpandFlush = () => {
  expandFlushPending = true;
  schedulePostLayoutRefit();
  // snapExpandedView's final settle is setTimeout(260). Keep the exemption
  // through that window so late post-layout refits still resize the grid/PTY
  // while the keyboard is open; clear on the next frame after settle.
  const EXPAND_FLUSH_MS = 280;
  setTimeout(() => {
    if (disposed) {
      expandFlushPending = false;
      return;
    }
    schedulePostLayoutRefit();
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        expandFlushPending = false;
      });
    });
  }, EXPAND_FLUSH_MS);
};
```

Store/clear the timer in `cancelExpandedSnap` / dispose cleanup the same way
`snapTimer` is handled (add `expandFlushTimer` next to `snapTimer`) so unmount
or a second expand toggle cannot leave a stale clear.

Do NOT change the expand `onclick` order (`beginExpandFlush` before
`snapExpandedView`).

Do NOT widen the ordinary keyboard-open guard.

## 8. Verification commands

```
npm run web:test -- --run src/components/TerminalRawView.test.ts -t "settle window while the keyboard is open"
npm run web:test -- --run src/components/TerminalRawView.test.ts -t "expand even while the keyboard is open"
npm run web:test -- --run src/components/TerminalRawView.test.ts
npm run web:check
npm run web:build
```

## 9. Acceptance criteria

- New settle-window test fails before impl, passes after.
- Existing keyboard-open expand test still passes.
- Full TerminalRawView suite green; web:check clean; dist regenerated via build.
- Diff limited to Allowed files.
- After expand+keyboard-open, a late proposed-dimension change within ~280ms
  still produces a grid+PTY resize.

## 10. Stop conditions

- Anchor missing / already uses a settle-duration clear → stop and report.
- New test passes before production edit → stop and report.
- Required edit needs files outside Allowed → stop.
- Unrelated test failures → report, do not "fix" by weakening tests.
