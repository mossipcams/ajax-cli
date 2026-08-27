# ACP session chat scroll — Agent of Empires StructuredView model

**Status:** Approved — user requested immediate implementation (2026-08-26).

## Scope

- Replace `column-reverse` + `margin-top: auto` first-paint trick with a
  chronological transcript column (oldest at top).
- Pin to the live edge via programmatic `scrollTop = scrollHeight - clientHeight`
  on first paint, live appends, and layout while pinned (AoE StructuredView +
  assistant-ui autoScroll semantics without adding `@assistant-ui/react`).
- Sample pin on user scroll into `pinnedRef` with 16px slop before layout settles.
- Composer `ResizeObserver` re-pins only when the pre-resize sample was pinned.
- Keyboard band stays on the flex root via `sessionKeyboardPadding` /
  `useMobileKeyboard` — no double-application.
- History read position: `scrollTop + scrollHeight` delta on prepend-style top
  growth; bottom append while unpinned does not yank.
- Recent-first paint window (`DEFAULT_HISTORY_WINDOW` = 150 rows): reducer keeps
  full transcript; Conversation paints a capped slice; scroll-up reveals
  already-held rows (no host before-cursor paging).
- Rewrite tests that encoded the column-reverse / no-scrollTop-first-paint contract.

## Non-goals

- `@assistant-ui/react` dependency
- Terminal live-view scroll model
- ACP protocol or task lifecycle changes
- Host `before=` cursor paging (cold attach already full-replays JSONL)
- dist build artifacts

## Checklist

- [x] CSS: chronological `.session-thread`, remove column-reverse and first-child margin-top auto
- [x] `sessionViewport`: scrollTop-based pin/slop (16px), stick-to-bottom
- [x] `useChatScroll`: first paint + session change pin, scroll sampling, observers while pinned
- [x] `useChatViewport`: composer resize re-pin only when `pinnedRef`
- [x] `historyScroll`: prepend delta helper, auto-load decision, stale-anchor drop
- [x] `historyWindow` + `useHistoryWindow`: 150-row first paint, turn-boundary snap, reveal growth
- [x] `ChatScroller`: Load earlier control + auto-load wiring
- [x] `web-session-behavior.md` transcript window + scroll contracts
- [x] Tests for window/grow/auto-load/restore contracts
- [ ] Manual iOS Safari keyboard/composer verification (operator)

## Files

- `crates/ajax-web/web/src/styles/chat/scrolling.css`
- `crates/ajax-web/web/src/shared/lib/sessionViewport.ts`
- `crates/ajax-web/web/src/features/chat/scrolling/useChatScroll.ts`
- `crates/ajax-web/web/src/features/chat/scrolling/useChatViewport.ts`
- `crates/ajax-web/web/src/features/chat/scrolling/historyScroll.ts`
- `crates/ajax-web/web/src/features/chat/conversation/historyWindow.ts`
- `crates/ajax-web/web/src/features/chat/conversation/useHistoryWindow.ts`
- `crates/ajax-web/web/src/features/chat/ChatSurface.tsx`
- Tests and `docs/architecture/web-session-behavior.md`

## Approval

User explicitly requested immediate implementation in the parent task — no separate architecture gate.
