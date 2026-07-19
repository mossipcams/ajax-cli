# Slice 5 — Strict Mode lifecycle safety

Master plan: `react-migration-cleanup.md`
Depends on: slice 4c (`777d41e`), Svelte guard (`293fbe8`)

## Measurement done before planning

Probes run against the real tree, not assumed:

1. **Playwright runs the Vite dev server**, not a production build
   (`playwright.config.mts:22`). StrictMode's double-invoke therefore **does**
   apply in e2e — this slice is genuinely testable.
2. **The deployed app is unaffected.** `dist` is a production build where
   `StrictMode` is an inert wrapper. **No iPhone risk from this slice.**
3. **The App shell is already StrictMode-safe.** Probe: cockpit fetches under
   `<StrictMode>` = **1**, not 2 — `createInFlightGuard` already collapses the
   double invoke. Slices 2c/4 hardened this incidentally.
4. **Exactly 4 e2e tests fail with StrictMode enabled**, 88 pass. All four are in
   `terminal-behavior.test.ts` and share **one** root cause.

## Root cause — not a disposal leak

```
task route mounts one terminal surface and opens one socket
  Expected length: 1
  Received length: 2
  [{"readyState": 3, …/terminal"}, {"readyState": 1, …/terminal"}]
```

`readyState: 3` is CLOSED. **The first socket is properly disposed** — cleanup
already works. The failures are all *counting* assertions over total constructed
sockets:

| Test | Asserts |
| --- | --- |
| `:385` mounts one terminal surface and opens one socket | total === 1 |
| `:1600` typing after manual reconnect sends exactly one input frame | total === 2 |
| `:1631` seeded reconnect restores live follow | total === 2 |
| `:2566` pty output corpus during delayed socket open | total === 1, `[0].readyState === 0` |

## The constraint that decides the approach

`docs/react-migration-plan.md:135` **D10**: *"Playwright e2e = frozen
characterization layer (edited only to add coverage, never edited to accommodate
a slice)."*

Changing these four to count *active* sockets would be defensible on its own
terms — the brief's wording is "only one terminal socket **remains active**", and
the repo already has an `activeTaskSocketCount` helper. **But D10 forbids it.**

So the code must satisfy the existing assertions: **total constructed sockets
must remain 1 under StrictMode.**

## Approach — defer the dial past the aborted first mount

StrictMode runs setup → cleanup → setup synchronously. If socket construction is
deferred by one frame and cancelled in cleanup, the aborted first mount never
constructs one, and total stays 1. This is a standard StrictMode-safe pattern and
keeps e2e untouched.

Anchor: `TaskTerminal.tsx:1013` `connectTaskTerminal(handle, { … })`.

Rejected alternative: editing the four assertions (violates D10).
Rejected alternative: disabling StrictMode (explicitly forbidden by the brief).

## Delegation decision

`Delegation decision: delegated via model-router`
Packet: `.planning/packets/react-cleanup-s5-strict-mode.md`

## Baselines

- Suite: **363 tests / 40 files**
- mobile-webkit e2e: **92 passed / 2 skipped** (StrictMode off)
- Target: **92 passed** with StrictMode on, e2e files unmodified

## Result — clean delegation

The fix is four lines of substance at `TaskTerminal.tsx:1013`: defer the dial
with `queueMicrotask`, guard it with the existing `disposed` flag (declared :429,
set in cleanup :1078), and make disposal `connection?.dispose()` for the case
where cleanup runs before the dial. StrictMode's aborted first mount now never
constructs a socket, so total-constructed stays 1 and the frozen assertions hold.

Causal evidence, not correlation: the parent measured the **same four tests**
failing before the fix and passing after, nothing else changed.

| Check | Result |
| --- | --- |
| mobile-webkit e2e | **92 passed / 0 failed** (was 88/4 with StrictMode on) |
| **e2e files modified** | **none** — D10 respected |
| `verify` | exit 0 — 41 files / **366 tests**, 1628 Rust tests |
| `web:lint` / `web:check` | 0 / 0 |

## Tests added beyond the packet

Nothing in vitest would have caught StrictMode being quietly removed from
`main.tsx` later — and deleting it is the fastest way to make a future terminal
test go green. `src/strictMode.test.tsx` adds:

1. entry point wraps in `<StrictMode>` — **proven by removal**: deleting the
   wrapper fails this test
2. no double cockpit fetch on a StrictMode double mount (locks in the
   in-flight-guard behaviour measured during planning)
3. focus listeners added == removed across a StrictMode mount/unmount

## Consequence for slice 10

The terminal effect **disposes correctly today** — established during planning
(first socket reaches `readyState: 3`, CLOSED) and reinforced by this fix.
Slice 10's controller extraction is therefore a **structural refactor, not a leak
hunt**. The four e2e tests here are its regression gate.

## Parent tooling error during this slice

The 3-minute progress poller reported `e2e-touched-lines=1` and the parent flagged
a possible D10 violation. That was the **monitor's bug**: RTK's git wrapper prints
an `ok` placeholder for empty output and `wc -l` counted it. Verified with
`rtk proxy` (unfiltered) — e2e was untouched throughout. Correct tripwire is
`rtk proxy "git diff --name-only <path>"`, not `git diff --stat | wc -l`.

## Deviations

- Added `src/strictMode.test.tsx` (not in the packet) so the wrapper cannot be
  silently removed.
