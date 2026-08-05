# Cross-session actionable banners (Ajax Web Session)

## Scope

Mobile-first cross-session banners on Ajax Web Session only (`ajax.webSession` + Cursor).
Direct ACP replies via process-local `WebSessionHub`. Park permission/question; fan out to other sockets.

## Non-goals

App-wide/terminal banners; Pi/Claude/Codex session; classic PWA; push/ntfy changes; cockpit inbox replace.

## Delegation decision

`Delegation decision: not delegated because R-SIZE-SPLIT` — full feature spans hub + protocol + UI across five waves (~400+ LOC). Parent implements sequential waves from the approved architecture plan for coherence; one bounded wave at a time with focused validation.

## Checklist

- [x] Wave 1: Park ACP permission/question; same-socket attention wire; drop auto-allow
- [x] Wave 2: WebSessionHub on WebState; multi-subscriber; cross-handle fan-out; grace TTL
- [x] Wave 3: SessionAttentionBanner + transport/types
- [x] Wave 4: Cockpit-derived review/failed; Open navigate; Stop/Retry via hub
- [x] Wave 5: Harden stale ids, reconnect pending replay, keyboard-band, docs

## Validation

```bash
cargo nextest run -p ajax-web web_session   # 18 passed
cargo nextest run -p ajax-web suite_4 park_permission web_session_hub  # 33 passed
npm run web:test -- --run src/features/session …keyboardBandPin …ajaxWebSessionSetting  # 36 passed
npm run web:test -- --run TaskDetail.test.tsx  # 31 passed
cargo check -p ajax-web  # clean (after architecture hub registration)
```

## Deviations

- Failed Stop/Retry banners come from hub-published ACP errors (not cockpit Error alone), matching Direct ACP requirement.
- Review Open is cockpit-derived; server ack is optional clear.

## Approval

User approved plan and asked to implement.
