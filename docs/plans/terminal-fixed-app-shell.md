# Terminal Fixed App Shell Plan

## Scope

Implement the requested Web Cockpit terminal-page behavior:

1. Add/confirm `viewport-fit=cover`.
2. Make the terminal page a fixed app shell.
3. Disable body/page scroll while the terminal page is open.
4. Use `100dvh` / VisualViewport sizing for the shell.
5. Put real controls in safe-area padding.
6. Make the terminal area internally scrollable.
7. Add a bottom input composer and quick-key toolbar.

Existing notes from inspection:

- `crates/ajax-web/web/app.html` already includes `viewport-fit=cover`.
- `crates/ajax-web/web/src/viewport.ts` already syncs `visualViewport.height` into `--app-height`.
- `TerminalRawView.svelte` already owns the raw xterm/tmux terminal, reconnect, resize, touch scrollback, sticky Ctrl, paste, hide-keyboard, and quick-key toolbar.
- The work should stay in `ajax-web` presentation code and preserve raw terminal behavior.

## Task 1: Lock the task route as a fixed viewport shell

- Failing behavior test to write:
  - Update `crates/ajax-web/web/src/components/TaskDetail.test.ts` with source-level assertions that mobile task view CSS uses a fixed shell with `inset: 0`, `height: var(--app-height, 100dvh)`, `height: 100dvh` fallback where appropriate, `overflow: hidden`, and safe-area padding.
  - Assert `html.ajax-task-open, html.ajax-task-open body` disables page scrolling.
- Code to implement:
  - Tighten `crates/ajax-web/web/src/styles.css` mobile task-shell rules so the terminal task route is a fixed app shell sized by `--app-height` with `100dvh` fallback and no document/body scroll.
  - Keep desktop dashboard/page scrolling unchanged.
- Verification:
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TaskDetail.test.ts`

## Task 2: Make terminal content the internal scroll/flex area

- Failing behavior test to write:
  - Update `crates/ajax-web/web/src/components/TerminalRawView.test.ts` with source-level assertions that the terminal panel has a stable shell layout, the terminal host is the flexing internal area, and xterm scrollback remains owned inside the terminal instead of the page.
- Code to implement:
  - Adjust `TerminalRawView.svelte` markup/CSS as needed so the panel is a column shell: terminal viewport flexes and the control composer/toolbar remains pinned at the bottom.
  - Preserve existing xterm scrollback interception through `term.scrollLines()` and horizontal panning.
- Verification:
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalRawView.test.ts`

## Task 3: Add the bottom input composer above the quick-key toolbar

- Failing behavior test to write:
  - Add tests in `crates/ajax-web/web/src/components/TerminalRawView.test.ts` that render a composer with a textbox and a Send button, send typed text plus Enter over the raw terminal socket, clear the composer after send, and leave empty sends inert.
  - Assert the existing quick-key toolbar remains present below/with the composer and includes real buttons.
- Code to implement:
  - Add composer state and send handling in `TerminalRawView.svelte`.
  - Place the composer in the terminal bottom controls area with safe-area-aware padding.
  - Send composer content as terminal input followed by carriage return through the same raw socket JSON input path used by xterm.
  - Keep existing quick keys (`Esc`, `Tab`, `Ctrl`, arrows, paste, hide keyboard, expand).
- Verification:
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/components/TerminalRawView.test.ts`

## Task 4: Final validation

- Failing behavior test to write:
  - None; this is validation-only after the focused TDD tasks are green.
- Code to implement:
  - None unless validation exposes implementation issues.
- Verification:
  - `rtk npm run web:check`
  - `rtk npm run web:test -- --run`
  - If time allows and the frontend checks pass, run the broader required Rust validation commands listed in `AGENTS.md`.
