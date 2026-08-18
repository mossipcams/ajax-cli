# Ajax chat two defects

## Problem

1. Orchestration chat refuses ordinary pasted prompts as too large because the
   session WebSocket frame ceiling is 4096 bytes.
2. Keyboard open/close jumps transcript position or flashes a keyboard-sized
   blank gap instead of restoring pre-transition geometry.

## Approval

User requested immediate implementation of both fixes.

## Scope

- Session WebSocket prompt frame limit (host + browser) and its tests
- Session chat keyboard/transcript scroll restoration and mobile WebKit coverage
- `docs/architecture/web-session-behavior.md` Mobile keyboard band (scroll
  restore contract only)

## Non-goals

- ACP protocol, transcript compaction, or context-window summarization
- Terminal keyboard / visualViewport ownership
- Commits or PRs unless the user asks

## Tasks

- [x] Open GitHub issues: #929 (prompt too large), #930 (keyboard scroll)
- [x] Raise host + browser session prompt frame ceiling; keep a hard refuse
      path that does not queue or poison the outbox
- [x] Regression: prompt > 4KB succeeds; still-oversized prompt is refused
- [x] Capture pre-transition transcript geometry on keyboard open/close
- [x] Restore equivalent position after layout settles (no animation)
- [x] Ignore Safari resize-generated scroll as user scrolling
- [x] Mobile WebKit regression: dismiss while pinned and while scrolled up
- [x] Update Mobile keyboard band contract if the restore rule changes
- [x] Pinned follow: thread growth after keyboard dismiss keeps live edge without
      a blind 500ms pin loop

## Acceptance

- Ordinary long pastes send; pathological sizes still refuse without poisoning.
- Keyboard transitions restore bottom vs history without a blank gap or a
  blind `scrollTop = scrollHeight` on every chat. During layout settle after
  keyboard close, pinned/live-edge restores pin `scrollTop = scrollHeight` each
  frame; history mode waits for the single post-settle restore.

## Verification

- [x] CI Web job skips Playwright `apt --with-deps` when browser cache hits
      (`fix(ci): skip Playwright apt deps on cache hit`)
- [x] `npm run web:test -- --run` transport + sessionViewport + useSessionChatViewport + SessionChat: 61 passed
- [x] `cargo test -p ajax-web max_session_frame_bytes`: pass (256 KiB)
- [x] Playwright `mobile-webkit` `session-chat-keyboard.test.ts`: 12/12 passed
- Approval: accepted after parent review of the delta
