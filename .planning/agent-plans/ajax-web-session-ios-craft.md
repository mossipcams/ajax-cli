# Ajax Web Session iOS PWA craft pass

## Problem

Console-gutter redesign fails on phone: cramped columns, mono walls,
uppercase chrome, poor thumb reach.

## Direction (Impeccable adapt + ios + Operate)

iOS Home Screen / Safari first inside Cockpit tokens:
- Shell inset 20px, spacing scale only
- Sans body ≥15–16px; mono only for symbols/chips
- Stacked conversation (role caption + body), not grid gutters or candy bubbles
- Composer: Messages-like row (Context | input | Send), 44px targets, 16px input
- Quiet header matching task detail, not tracked SESSION scream

## Delegation

`not delegated because visual craft / iOS layout coherence`

## Checklist

- [x] Markup + CSS iOS PWA pass
- [x] DESIGN.md surface note
- [x] detect.mjs + tests + web:build
- [x] Attention: temporary fixed top toast; tap opens task; remove Open button

## Validation

- SessionAttentionBanner + keyboardBandPin: 14 passed
- web:check / web:build: pass
- detect: session-local advisories cleaned (pill tokens); pre-existing bounce-easing elsewhere ignored
