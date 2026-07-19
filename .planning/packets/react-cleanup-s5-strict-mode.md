# TDD implementation packet — slice 5: Strict Mode lifecycle safety

PACKET_STATUS: READY
TEST_FIRST: NOT_APPLICABLE (four existing e2e tests are the failing test)
PRODUCTION_EDIT: REQUIRED

## Task contract

Enable `StrictMode` in `main.tsx` and make the terminal mount effect safe under
React's development setup → cleanup → setup cycle, so the **existing, unmodified**
e2e suite passes.

One bounded outcome: `npm run web:smoke -- --project=mobile-webkit` reports
**92 passed** with StrictMode enabled and **zero changes to any e2e file**.

## Allowed files

- `crates/ajax-web/web/src/main.tsx`
- `crates/ajax-web/web/src/components/TaskTerminal.tsx`
- `crates/ajax-web/web/src/terminalConnection.ts` (only if unavoidable — say so)

## Forbidden changes

- **Do not edit any file under `crates/ajax-web/web/e2e/`.** This is a hard repo
  rule (`docs/react-migration-plan.md:135`, decision D10): the Playwright suite is
  a frozen characterization layer, edited only to *add* coverage, never to
  accommodate a slice. Editing an assertion here is an automatic reject.
- **Do not disable or conditionally skip StrictMode**, and do not gate it on an
  env var. It goes on, unconditionally.
- Do not weaken, skip, or retarget any socket-cardinality assertion.
- Do not touch `App.tsx`, the resource hooks, or any other component.
- Do not add `eslint-disable` anywhere.
- **Never redirect command output into `/tmp`** — auto-rejected, kills your round.

## Established facts — do not re-derive these

Measured by the parent against this tree:

1. Playwright runs the **Vite dev server** (`playwright.config.mts:22`), so
   StrictMode double-invoke applies in e2e.
2. With StrictMode on: **88 pass, 4 fail**, all in `terminal-behavior.test.ts`.
3. The four failures share one cause. The first socket reaches
   `readyState: 3` (CLOSED), so **disposal already works** — this is not a leak.
   The assertions count *total constructed* sockets:

| Test | Expects |
| --- | --- |
| `:385` mounts one terminal surface and opens one socket | total 1, got 2 |
| `:1600` typing after manual reconnect sends exactly one input frame | total 2, got 3 |
| `:1631` seeded reconnect restores live follow | total 2, got 3 |
| `:2566` pty output corpus during delayed socket open | total 1 and `[0].readyState === 0` |

4. The App shell is already StrictMode-safe (cockpit fetches under StrictMode = 1).
   **Do not change shell or polling code.**

## Required approach

Because e2e cannot change, the code must keep **total constructed sockets at 1**.

StrictMode runs setup → cleanup → setup synchronously. Defer the dial so the
aborted first mount never constructs a socket, and cancel that pending dial in
cleanup:

- Anchor: `TaskTerminal.tsx:1013` — `const connection = connectTaskTerminal(handle, { … })`.
- Schedule the connect on a microtask or animation frame; store the handle.
- In the effect's cleanup, cancel the pending dial if it has not run, and
  dispose the connection as it does today if it has.
- Everything the effect creates must still be torn down in cleanup: listeners,
  observers, timers, animation frames, xterm instance, fit addon.

If deferring by one frame proves impossible without changing observable
behaviour, **stop and report** with the specific obstacle. Do not fall back to
editing e2e.

## Behaviour that must not change

- Connect latency stays imperceptible; no visible "Connecting" flash that was
  not there before.
- Reconnect, seeded reconnect, and delayed-open flows behave as today — the three
  reconnect tests are the proof.
- No change to keyboard band, fullscreen, paste, copy, scroll follow, or geometry.

## Verification commands

```bash
npm run web:test -- --run
npm run web:check
npm run web:lint
npm run web:smoke -- --project=mobile-webkit
git diff --stat crates/ajax-web/web/e2e
```

- e2e: **92 passed**, 2 skipped, **0 failed**.
- `git diff --stat crates/ajax-web/web/e2e` must print **nothing**.
- Vitest: **40 files**, **at least 363 tests**, zero failures.
- `web:lint` and `web:check` exit 0.

## Stop conditions

- You cannot reach 92 e2e passes without editing an e2e file.
- Deferring the dial changes observable connect behaviour.
- Any vitest test regresses.
- You need to touch `App.tsx` or the resource hooks.
- The patch would exceed roughly 400 changed lines.
