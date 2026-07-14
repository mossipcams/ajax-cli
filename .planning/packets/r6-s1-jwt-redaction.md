# Packet R6 — S1: JWT-shaped API text must not reach the DOM

All paths relative to `crates/ajax-web/web`; run all commands from there.

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

If a JWT-shaped token (three dot-joined base64url segments, first decoding to
a `{"alg"…` JSON header — practically: `eyJ`-prefixed) appears in any API
response string the cockpit renders (`status_explanation`, `title`, or any
other display field), it must be redacted before it can reach the DOM. Svelte
already escapes interpolation (no XSS); this is display-redaction so tokens
are never shown or captured in `outerHTML`.

## 3. Allowed files

- `src/api.ts` — redaction applied at the fetch boundary
- `src/api.test.ts` — focused unit tests for the redaction helper/boundary

## 4. Forbidden changes

- No component/render-site changes (fix once at the boundary, not per caller).
- No change to request bodies, WS terminal frames, auth/session handling, or
  `contracts.ts` validation semantics.
- No e2e file edits.

## 5. Context evidence

- **Graphify:** NOT_REQUIRED — single-boundary change in the browser API
  client; no task-truth/lifecycle surface touched.
- **Serena:** NOT_REQUIRED — anchors collected by direct read; one file.
- **ast-grep:** NOT_REQUIRED — exact function anchors below.

## 6. Code anchors

`src/api.ts`:

- `readJson(response)` :76 — sole JSON parse point for all API reads.
- `getJson` :141–147 → `fetchCockpit` :149 (`assertCockpit`), `fetchDetail`
  :154 (`assertDetail`) — the two payloads whose strings the UI renders.
- `postJson` :164–174 → `postMutation` :196–205 — operation responses carry a
  refreshed cockpit projection (`assertOperationResponse`), so mutations must
  get the same redaction.

E2E ground truth: `e2e/jwt-adversarial.test.ts` plants the canary
`eyJhbGciOi…` in `status_explanation` and `title` (cockpit + detail fixtures)
and fails when it appears in `document.documentElement.outerHTML`
(`jwtLeakScan.ts` surface "html"). Terminal WS output is canvas-rendered and
excluded from the DOM surface; do not touch the WS path.

## 7. Test-first instructions

1. New focused unit test in `src/api.test.ts` (red first):
   - `redactJwts` (exported helper) replaces every JWT-shaped substring
     (`eyJ` + base64url, two more dot-joined base64url segments, each segment
     ≥ 4 chars) with `[redacted]` in nested objects/arrays of a payload;
   - non-JWT text, URLs, and ordinary base64 fragments pass through unchanged;
   - a cockpit-shaped payload with the canary in `cards[0].status_explanation`
     comes out redacted via the fetch boundary (mock `fetch`).
   RED command: `npx vitest run src/api.test.ts`
2. Existing failing e2e (red before, green after):
   `npx playwright test e2e/jwt-adversarial.test.ts --project=desktop-chromium`

## 8. Edit instructions

- Add a small pure `redactJwts(value: unknown): unknown` in `src/api.ts`
  (deep walk: strings redacted via one regex, arrays/objects mapped, other
  types untouched).
- Apply it once where all rendered payloads pass: the return of `readJson`,
  or — if error strings must stay raw — wrap the three assert call sites
  (`assertCockpit`, `assertDetail`, `assertOperationResponse` inputs).
  Prefer the single `readJson` choke point unless a unit test shows a
  round-trip field must keep raw tokens (none known).
- Keep the regex conservative: `eyJ[A-Za-z0-9_-]{4,}\.[A-Za-z0-9_-]{4,}\.[A-Za-z0-9_-]{4,}`
  (JWT header always base64url-encodes `{"` as `eyJ`).

## 9. Verification commands

```bash
npx vitest run src/api.test.ts
npx playwright test e2e/jwt-adversarial.test.ts --project=desktop-chromium
npx playwright test e2e/jwt-adversarial.test.ts --project=mobile-webkit
npx playwright test e2e/jwt-leak.test.ts --project=desktop-chromium
npx vitest run
```

## 10. Acceptance criteria

- New unit tests shown red first, then green.
- Both jwt-adversarial projects pass; jwt-leak stays green.
- Full vitest suite passes.
- Diff confined to `src/api.ts` + `src/api.test.ts`.

## 11. Stop conditions

- Redaction at the boundary breaks a contract test expecting raw values
  (would force per-field redaction — stop and report instead).
- The e2e still fails because the canary reaches the DOM via a non-API path
  (report the path; do not chase it into forbidden files).
- Patch would exceed ~120 changed lines.
