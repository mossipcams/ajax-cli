# Xterm implementation code review

## Verdict

**Changes requested.** The delegated implementation passes PR 510's 27
mobile-WebKit cases, but the implementation and PTY can disagree about geometry
and several documented product behaviors are not actually preserved.

## Findings

### P1 — xterm and the PTY disagree below 80 columns

**Resolved in implementation branch.** A permanent mobile-WebKit case now
proves the logical grid exceeds the phone host, scales to fill it, and reports
at least 80 columns; the PTY receives the actual xterm dimensions.

`TaskTerminal.svelte:174-183` sends `max(term.cols, 80)` to the PTY, while
`TaskTerminal.svelte:186-187` and `:393` leave the local xterm grid at the
narrower FitAddon result. On a phone, tmux therefore wraps at 80 columns while
xterm can wrap at roughly 40-50 columns. The renderer must use the same logical
column count sent to the PTY and scale that grid to the host width.

### P1 — keyboard resize handling violates the discrete-intent exceptions

`TaskTerminal.svelte:203-216` still performs local fits during ordinary
keyboard-open viewport bursts, while `:174-177` blocks every PTY resize,
including the required pinch-end and expand-enter exceptions. Ordinary
keyboard events must freeze fit and PTY resize; explicit pinch/expand intent
must fit and resize once.

### P1 — seeded reconnect state is ignored

`TaskTerminal.svelte:422-426` ignores `onOpen(isReconnect, seeded)`. A manual
seeded reconnect can append fresh history to stale xterm contents and retain an
old unpinned scroll-follow state. Seeded reconnects must reset the local buffer,
follow/unseen state, and live position; automatic unseeded reconnects must keep
the buffer.

### P1 — Paste bypasses xterm paste semantics and fails silently

`TaskTerminal.svelte:130-136` sends clipboard text directly to the socket.
That bypasses xterm's bracketed-paste handling, and unavailable/denied clipboard
access exposes no native fallback or visible notice. Successful clipboard reads
must flow through `term.paste`; failure must expose the documented fallback.

### P1 — physical touch acceptance is not implemented or proven

The component handles pinch only (`TaskTerminal.svelte:330-368`). It has no
explicit long-press selection/copy flow and hides horizontal overflow without a
horizontal-pan path (`:561-569`). These are physical-iPhone product rows, not
covered by the 27 Playwright proxies. They remain a device-validation risk and
must not be reported as proven by automated CI.

### P2 — a post-layout frame survives disposal

`TaskTerminal.svelte:212-217` schedules an untracked nested animation frame,
but cleanup at `:438-465` cancels only `fitFrame`. Navigation during a
fullscreen/pinch settle can run fit work after xterm and FitAddon are disposed.

### P2 — toolbar focus semantics are mouse-only and unconditional

Toolbar controls use `mousedown` rather than `pointerdown` and always refocus
xterm (`TaskTerminal.svelte:493-539`). A touch can reopen a keyboard the user
intentionally hid. Preserve prior terminal focus and refocus only when the
terminal owned it; fullscreen exit should blur.

### P3 — undefined design token

`TaskTerminal.svelte:609`, `:661`, and `:685` use `--surface-raised`, while Ajax
defines `--paper-raised`. The affected controls lose their intended background.

## Review evidence

- Parent and delegated agents read `TaskTerminal.svelte`, `terminalConnection.ts`,
  `architecture.md`, `TERMINAL_BEHAVIOR_CONTRACT.md`, and
  `TERMINAL_REBUILD_ACCEPTANCE.md`.
- The 27-case suite and full repository validation were previously green, which
  demonstrates that the permanent proxy suite does not cover all findings.
- Proposed black-box RED additions cover keyboard-open expand resize,
  bracketed paste, and seeded reconnect follow restoration. Geometry and
  disposal need a focused component test against the xterm/FitAddon boundary.
