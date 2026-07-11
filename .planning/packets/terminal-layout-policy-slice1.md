# TDD Implementation Packet — terminal layout policy Slice 1

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal

Own fit/resize *permission* in a pure `terminalLayoutPolicy` module so
`TerminalRawView.svelte` no longer uses `pinchFlushPending` /
`expandFlushPending` / `expandFlushTimer`. Preserve existing keyboard freeze,
pinch-end exemption, and expand-settle exemption behavior.

## Allowed files

- `crates/ajax-web/web/src/terminalLayoutPolicy.ts` (new)
- `crates/ajax-web/web/src/terminalLayoutPolicy.test.ts` (new)
- `crates/ajax-web/web/src/components/TerminalRawView.svelte`
- `crates/ajax-web/web/TERMINAL.md`
- `architecture.md` (one short ownership pointer near existing TERMINAL.md ref)
- `crates/ajax-web/web/dist/*` (only via `npm run web:build`)

## Forbidden changes

- Do not edit `terminalRefit.ts` scheduling semantics (permission ≠ scheduling).
- Do not change Ghostty attach, WS protocol, scroll-follow, zero-lag, paste/copy.
- Do not weaken/delete existing `TerminalRawView.test.ts` cases.
- Do not add new `*FlushPending` booleans anywhere.
- Do not touch unrelated crates or root `tests/`.

## Context evidence

### Graphify

`NOT_REQUIRED`: ownership already documented in `TERMINAL.md` / `architecture.md`
Web Cockpit terminal section; this slice only adds the missing permission owner
named by the existing anti-pattern.

### Serena

`NOT_REQUIRED` for dispatch: Serena CLI in this environment only exposes
`tools list/description` and `project *`; symbol find/reference requires MCP.
Inventory completed with ast-grep + ripgrep instead (same anchors).

### ast-grep / rg inventory (pre-edit)

```text
TerminalRawView.svelte:
  pinchFlushPending set/clear: ~505–514, decl ~610
  expandFlushPending decl ~618; beginExpandFlush ~696–716; dispose ~994–998
  sendResize guard ~548: isKeyboardOpen() && !pinch && !expand → return
  fitNow guard ~633: keyboardOpen && !pinch && !expand → crop+return
  expand onclick ~1059–1062: beginExpandFlush(); snapExpandedView();
EXPAND_FLUSH_MS = 280 in beginExpandFlush
Pinch clear = double requestAnimationFrame after schedulePostLayoutRefit
```

## Code anchors

Current permission logic to replace (behavior oracle):

1. `sendResize`: withhold when keyboard open unless pinch/expand flush pending.
2. `fitNow`: same withhold → bottom-crop host + snap if pinned; on keyboard
   open edge, set `pinnedToBottom = true`.
3. `pinchEnded`: set pinch pending, `schedulePostLayoutRefit()`, clear after
   two rAFs.
4. `beginExpandFlush`: set expand pending, `schedulePostLayoutRefit()`, after
   280ms schedule another post-layout then clear after two rAFs.
5. Dispose: clear expand timer + expand pending.

Integration tests that must stay green (do not rewrite to pass):

- `"freezes the local grid while the keyboard is open..."`
- `"resizes the grid on expand even while the keyboard is open"`
- `"keeps expand flush through the settle window while the keyboard is open"`
- `"flushes the PTY resize when the pinch ends"`
- `"applies the pinch rewrap while the keyboard is open"`

## Test-first instructions

Create `terminalLayoutPolicy.test.ts` first. Tests must fail before production
module exists / before API is complete.

Required cases:

1. `keyboard closed → allowLocalFit and allowPtyResize true, cropToBottom false`
2. `keyboard open with no discrete intent → both allows false, cropToBottom true`
3. `keyboard open edge → pinToBottomOnKeyboardOpen true once, then false`
4. `pinchEnded while keyboard open → allows true until double-rAF clear; then freeze again`
5. `expandEnter while keyboard open → allows true through EXPAND_REWRAP_MS (280) + double-rAF clear`
6. `expandExit does not leave a stale expand intent`
7. `dispose clears pending timers/intents; later ticks are no-ops`

Inject `now`, `schedule`/`clearSchedule`, and `raf`/`cancelRaf` (fake timers /
manual queues) — mirror `createTerminalWriteBatcher` style.

Focused red command:

```bash
npm run web:test -- --run src/terminalLayoutPolicy.test.ts
```

## Edit instructions

### A. `terminalLayoutPolicy.ts`

Export:

- `EXPAND_REWRAP_MS = 280` (named constant; matches current expand flush)
- `createTerminalLayoutPolicy(options?)` returning:
  - `setKeyboardOpen(open: boolean): LayoutDecision`
  - `expandEnter(): LayoutDecision`
  - `expandExit(): LayoutDecision`
  - `pinchEnded(): LayoutDecision`
  - `decision(): LayoutDecision`
  - `dispose(): void`

`LayoutDecision`:

```ts
{
  allowLocalFit: boolean;
  allowPtyResize: boolean;
  cropToBottom: boolean;
  pinToBottomOnKeyboardOpen: boolean;
}
```

Rules:

- `allowLocalFit === allowPtyResize === (!keyboardOpen || discreteIntentActive)`
- `cropToBottom === (keyboardOpen && !discreteIntentActive)`
- Pinch intent: activate on `pinchEnded`; clear after two injected rAFs
- Expand intent: activate on `expandEnter`; after `EXPAND_REWRAP_MS`, run two
  rAFs then clear (permission lifetime only — component still schedules refits)
- `expandExit`: clear expand intent immediately if active
- `pinToBottomOnKeyboardOpen`: true only on false→true keyboard transition in
  that `setKeyboardOpen` / `decision` call; subsequent `decision()` false

### B. Wire `TerminalRawView.svelte`

- Create one policy instance in `onMount` (with real timers/raf).
- `pinchEnded` → `policy.pinchEnded(); schedulePostLayoutRefit();` (no local flags)
- `beginExpandFlush` → `policy.expandEnter(); schedulePostLayoutRefit();` plus
  keep the 280ms *refit* re-schedule that exists today OR move only permission
  to policy and keep a thin timer that only calls `schedulePostLayoutRefit`
  then relies on policy’s own clear — simplest: policy owns permission timer;
  component on expandEnter also `schedulePostLayoutRefit()`, and after
  `EXPAND_REWRAP_MS` calls `schedulePostLayoutRefit()` once more (can use
  policy callback or duplicate the 280ms schedule for refit only without
  setting flags). Prefer: policy exposes nothing for refit; component keeps:

  ```ts
  beginExpandFlush = () => {
    policy.expandEnter();
    schedulePostLayoutRefit();
    if (expandRewrapTimer) clearTimeout(expandRewrapTimer);
    expandRewrapTimer = setTimeout(() => {
      expandRewrapTimer = undefined;
      if (!disposed) schedulePostLayoutRefit();
    }, EXPAND_REWRAP_MS);
  };
  ```

  Import `EXPAND_REWRAP_MS` from the policy module. No `expandFlushPending`.

- `sendResize` / `fitNow`: use `policy.decision()` (or sync keyboard via
  `policy.setKeyboardOpen(isKeyboardOpen())` at start of `fitNow`/`sendResize`).
  Prefer calling `setKeyboardOpen(isKeyboardOpen())` once at the top of
  `fitNow` and reading the returned decision; `sendResize` calls `decision()`
  after ensuring keyboard state is current the same way.
- On keyboard-open edge from decision: set `pinnedToBottom = true` /
  `hasUnseenOutput = false` (same as today).
- Dispose: `policy.dispose()`; clear any expand rewrap *refit* timer.

### C. Docs

`TERMINAL.md` ownership table: add

`| Layout fit/resize permission | terminalLayoutPolicy.ts |`

Clarify refit row remains scheduling-only. Anti-pattern: no `*FlushPending` in
components.

`architecture.md` near the TERMINAL.md pointer (~665): one sentence that
layout fit/resize permission lives in `terminalLayoutPolicy.ts`.

## Verification commands

```bash
npm run web:test -- --run src/terminalLayoutPolicy.test.ts
npm run web:test -- --run src/components/TerminalRawView.test.ts
npm run web:check
npm run web:build
ast-grep --pattern '$XFlushPending' --lang ts crates/ajax-web/web/src
# expect: only terminalOwnership.test.ts / TERMINAL.md mentions if any; zero in .svelte production
rg -n 'FlushPending' crates/ajax-web/web/src
```

## Acceptance criteria

- New unit tests green; all existing TerminalRawView keyboard/expand/pinch tests green.
- Zero `pinchFlushPending` / `expandFlushPending` / `expandFlushTimer` in
  `TerminalRawView.svelte`.
- `TERMINAL.md` lists `terminalLayoutPolicy.ts` as permission owner.
- Diff limited to Allowed files.

## Stop conditions

- Any TerminalRawView behavior test requires weakening → stop and report.
- Need to change `terminalRefit.ts` semantics → stop.
- Scope creeps into scroll/echo/paste → stop.
- Two failed delegate rounds → stop.
