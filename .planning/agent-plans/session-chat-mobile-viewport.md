# Session chat mobile viewport ownership (#877)

## Problem

On iOS Safari/PWA, tapping the transcript dismisses the keyboard, but Ajax can
leave a keyboard-sized blank region between the last message and the composer.
The composer is docked; the transcript does not recompute available height or
live-edge position after the keyboard transition.

This is the remaining cause of
[#877](https://github.com/mossipcams/ajax-cli/issues/877). Isolated
`visualViewport` / `releaseKeyboardBand` patches have not closed it.

Architectural reference: Agent of Empires
`StructuredView` + `useMobileKeyboard` + `acp-composer-keyboard.spec.ts`.

## Approval

User requested immediate implementation of the ownership redesign. Do not add
another isolated visualViewport resize patch.

## Scope

Session chat viewport ownership and keyboard transitions only.

- `crates/ajax-web/web/src/features/session/`
- `crates/ajax-web/web/src/styles.css`
- `crates/ajax-web/web/src/shared/lib/viewport.ts` and tests
- `crates/ajax-web/web/src/shared/hooks/useViewportBand.ts` and tests
- `crates/ajax-web/web/src/app/AppViewport.tsx`, `RouteScroll.tsx`, session App tests
- WebKit mobile Playwright coverage
- `docs/architecture/web-session-behavior.md` (Mobile keyboard band)

## Non-goals

- ACP protocol, transcript data model, prompt queue
- Unrelated terminal behavior rewrites
- Task lifecycle / registry / runtime authority changes
- Commits or PRs unless the user asks

## Required layout model

```text
SessionChatSurface
├── LiveHead                 fixed flex child
├── TranscriptViewport       only scroll owner
└── Composer                 fixed flex sibling
```

- Session chat surface is a bounded flex column.
- Every ancestor from app viewport to session surface has `min-height: 0`.
- Transcript is the only vertical scroll container: `flex: 1 1 0`, `min-height: 0`, `overflow-y: auto`.
- Composer is `flex: none` outside the transcript scroller.
- Route-level page scroller must not compete with the transcript.
- No transcript-child `margin-top: auto` unless proven necessary.
- No transforms to move transcript or composer around the keyboard.

## Keyboard ownership

- Keep global `html.keyboard-open` / `--app-height` for task and terminal if still required.
- Scope or bypass that takeover for orchestration session chat if it double-owns geometry.
- One authoritative visible-height calculation for session chat.
- Support iOS Safari (`visualViewport` shrinks, `innerHeight` may not) and iOS PWA/Android (layout viewport may already shrink).
- Do not apply keyboard padding twice.
- Do not leave stale keyboard height, bottom padding, fixed positioning, or max-height after close.
- Preserve safe-area without a second blank strip below the composer.

## Scroll behavior

- Track live-bottom pin before keyboard/composer resize.
- Keyboard viewport resize is a layout change, not user scroll-up.
- If pinned, re-pin to `scrollHeight` after layout settles.
- If reading history, preserve visible position.
- ResizeObserver / rAF only after flex layout has settled.
- Do not assign `scrollTop` on every `visualViewport` event unless pinned.
- Handle open, close, and multiline composer growth with the same model.

## Tasks

- [x] Redesign session chat as bounded flex column with one transcript scroller.
- [x] Make session chat the owner of its mobile visible-height calculation.
- [x] Scope/bypass global keyboard-open takeover for session if it double-owns geometry.
- [x] Preserve pin vs history through keyboard and composer height changes.
- [x] Add unit + WebKit Playwright geometry assertions (not screenshots-only).
- [x] Update `docs/architecture/web-session-behavior.md` Mobile keyboard band.

## Verification

- Focused vitest: SessionChat, viewport, viewport-band, and any new session-viewport tests.
- Playwright `mobile-webkit`: new session chat keyboard geometry suite.
- Preserve existing session-chat regression and keyboard-band pin coverage.
- Do not declare fixed merely because the composer is visible.

## Acceptance

Keyboard dismissal settles transcript + composer into the same correct layout as
a fresh render at that viewport height, without losing the user’s scroll intent.
The composer remaining visible is not sufficient.

## Deviations

- Round 2: `useMobileKeyboard` clears keyboard band on composer blur without
  waiting for visualViewport resize (tap-dismiss often omits it); Playwright
  tap-dismiss and streaming tests no longer call a helper that restores vv.
