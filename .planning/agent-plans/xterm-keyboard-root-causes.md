# Root-cause follow-up — keyboard textarea + expand rewrap (keep full-bleed)

## Scope

PR #515’s full-bleed width fix was the only change that hit a real root cause.
This follow-up fixes the remaining defects by porting the Ghostty behaviors that
were dropped in the xterm rewrite:

1. **Status box** — mobile `.interact-panel` must be a single compact row (not
   multi-line wrap + padding sprawl). Text ellipsis alone was insufficient.
2. **Inline + keyboard** — iOS places the keyboard relative to the focused
   textarea. xterm still parks `.xterm-helper-textarea` at `left:-9999em;top:0`.
   Root fix: bottom-anchor + soften (Ghostty `hardenMobileTextarea`).
3. **Fullscreen + keyboard** — iOS animates `visualViewport` for ~280ms after
   expand; a single `schedulePostLayout(true)` reads pre-animation geometry and
   leaves the bottom clipped. Root fix: expand settle window (rAF ×2 + 280ms)
   with discreteIntent refits while keyboard-open.
4. **Keep top chrome** — offscreen textarea focus makes Safari scroll-chase away
   from the header. Bottom-anchoring + `resetDocumentScroll` before focus is the
   root fix; keep `keyboard-open` from hiding `.detail-header`/`.interact-panel`
   (already on main).

## Non-goals

- Do not revert or rework the full-bleed task route padding (that worked).
- No Ghostty restore, architecture, PTY protocol, or auth changes.
- No CSS-only “flex fill” theatre without the textarea/expand settle roots.

## Delegation decision

`Delegation decision: delegated via model-router` → Cursor / composer-2.5

## Task checklist

- [x] Packet: `.planning/agent-plans/packets/xterm-keyboard-root-causes.md`
- [x] RED→GREEN: textarea bottom-anchor contracts + expand settle while keyboard-open
- [x] Impl: `TaskTerminal.svelte` harden + settle; `TaskDetail.svelte` single-row panel
- [x] Verify + follow-up PR

## Validation ledger

- RED: TaskTerminal/TaskDetail new contracts failed as intended
- GREEN: 23/23 focused tests; `web:check` PASS; smoke keyboard/expand 6/6 PASS
- Full-bleed `padding-left: 0` retained in `styles.css`
- Review gate: **ACCEPT** (textarea harden + EXPAND_REWRAP settle + single-row panel)
