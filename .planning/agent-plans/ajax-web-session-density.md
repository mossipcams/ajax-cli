# Ajax Web Session density + alignment

## Problem

Session UI still feels oversized and misaligned: 20px side inset vs task
chrome’s 12px, wide nearly-full bubbles, heavy role captions, loose turn gap.

## Spatial thesis

- Primary path: transcript → composer → key bar
- Align session content to task detail inset (`12px` + safe-area)
- Dense Operate chat: tight turn gap, hug-content bubbles, user ≤~78% width
- Keep 44px tap targets and 16px composer input (iOS zoom)

## Non-goals

- No backend / protocol / banner behavior changes
- No return to console gutters
- No classic PWA packaging

## Delegation

`Delegation decision: not delegated because visual craft / layout coherence`

## Checklist

- [x] CSS density + shared 12px inset; bubble max-widths / hug content
- [x] Quiet role meta (drop “You”; keep Cursor / Streaming lightly)
- [x] DESIGN.md surface note
- [x] Focused session tests + web:check + web:build

## Validation

- AjaxWebSessionView + keyboardBandPin: 17 passed
- web:check / web:build: pass
- detect layout: clean

