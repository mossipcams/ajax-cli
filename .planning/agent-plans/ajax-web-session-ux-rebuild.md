# Ajax Web Session UX Rebuild

## Scope

iOS-Safari-first Operate rebuild of Ajax Web Session behind `ajax.webSession`:
shell UX, literal terminal hotkey bar (composer-wired), keyboard-band pin,
transport reconnect/Retry, error clarity. Keep banners, symbols, Cursor ACP hub.

## Non-goals

Non-Cursor agents, classic PWA packaging, terminal path when flag off,
browser-owned task truth, ACP surface expansion unless reliability blocked.

## Delegation decision

`not delegated because R-SIZE-SPLIT / multi-wave coherence` — parent implements
sequential waves locally; estimated size exceeds one bounded packet.

## Checklist

- [x] Wave 0 — Impeccable Operate brief + keyboard-open layout contract
- [x] Wave 1 — Session shell rebuild (states, transcript, composer dock, CSS)
- [x] Wave 2 — Hotkey bar + Mic→draft + keyboard-band pin tests
- [x] Wave 3 — Transport reconnect/backoff/Retry + hub grace verify
- [x] Wave 4 — Error copy + web-cockpit.md + focused validation

## Approval

User approved attached plan (1A hotkeys, 2B/C reliability).

## Deviations

- Hub grace reattach: verified in existing `get_or_create_slot` (reuse peer when
  present) + pending replay on `attach`; no new Rust test (ACP spawn seam
  required for full integration). No ACP protocol change needed.
- Speech for Mic: new `useSessionComposerSpeech` (append to draft) rather than
  extracting a shared helper from TaskTerminal (keeps terminal PTY path intact).

## Validation

- `npm run web:test -- --run …/session …/keyboardBandPin.test.ts` → 40 passed
- `npm run web:check` → pass
- `npm run web:lint` → pass (prior run)
- `npm run web:build` → pass (dist refreshed; prior run)
- `cargo nextest run -p ajax-web web_session_hub` → 2 passed (prior run)
- Re-validated 2026-08-05: session + keyboardBandPin 40 passed; web:check pass
