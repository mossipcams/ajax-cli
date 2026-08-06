# Ajax Web Session: supervision surface (not chat)

## Mode

Behavior Change — Wave 1 UX remap (confirmed shape brief). **COMPLETE**

## Scope

Supervision card feed + hidden composer + header Stop. Existing wire only.

## Non-goals

- Backend ACP enrichment / structured progress DTOs
- Ship action (no session API yet)
- SessionAttentionBanner remote-toast redesign
- PWA packaging / terminal-default replacement

## Confirmed brief

1. Both live card feed and quiet status + Stop while running
2. UX-only this wave
3. Composer fully hidden until Redirect / question free-text

## Delegation decision

`Delegation decision: delegated via model-router` — Wave 1 packet
`.planning/router-runs/ajax-web-session-supervise-w1/packet.md`
Lane: `cursor-delegate` / `composer-2.5` / compact.
Review Gate: **ACCEPT** (delegate report schema failed; parent verification passed).

## Checklist

- [x] Shape brief confirmed
- [x] Wave 1 packet written
- [x] Packet check + dispatch
- [x] Review Gate ACCEPT
- [x] Parent validation (web:test / web:check / web:build)
- [x] DESIGN.md supervision blurb
- [x] Record codebase-intel decision (#13)

## Validation

```text
npm run web:test -- --run …AjaxWebSessionView …SessionAttentionBanner …sessionCards
→ 18 passed
npm run web:check → pass
npm run web:build → pass
```

## Remaining risks / follow-ups

- Ship action not wired (needs session/cockpit prop)
- Diff churn high (~958) for a single wave; coherent but large
- Progress cards are truncated assistant text, not structured tool/diff events
- Detector advisories on preexisting CSS radii/colors outside DESIGN.md
