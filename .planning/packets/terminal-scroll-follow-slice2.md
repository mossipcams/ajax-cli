# TDD Implementation Packet — terminal scroll-follow Slice 2

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal

Own pinned-to-bottom / unseen-output *state transitions* in
`terminalOutputPolicy.ts` via `createScrollFollowPolicy`, so
`TerminalRawView.svelte` stops scattering `pinnedToBottom` /
`hasUnseenOutput` mutations. Keep existing pure helpers
(`outputFollowEffects`, `scrollbackGrowthCompensation`).

## Allowed files

- `crates/ajax-web/web/src/terminalOutputPolicy.ts`
- `crates/ajax-web/web/src/terminalOutputPolicy.test.ts`
- `crates/ajax-web/web/src/components/TerminalRawView.svelte`
- `crates/ajax-web/web/TERMINAL.md` (sharpen scroll-follow row only if needed)
- `crates/ajax-web/web/dist/*` (only via `npm run web:build`)

## Forbidden changes

- Do not edit `terminalLayoutPolicy.ts` / layout wiring.
- Do not change zero-lag, paste/copy, gestures math, or WS protocol.
- Do not weaken/delete existing TerminalRawView scroll/follow tests.
- Do not move write batching or resize dedupe ownership.
- No new dependencies; no commits/branch changes.

## Context evidence

### Graphify
`NOT_REQUIRED`: TERMINAL.md already assigns scroll-follow to
`terminalOutputPolicy.ts`.

### Serena
`NOT_REQUIRED`: CLI has no symbol find; inventory via rg/ast-grep.

### ast-grep / rg inventory

`TerminalRawView.svelte` mutates follow state at:
- decl `pinnedToBottom = true` (~242); `hasUnseenOutput` $state (~90)
- scroll gesture → `pinnedToBottom = false` (~496)
- jumpToBottom → clear unseen (~570–572)
- keyboard-open edge / snapVisible / reconnect → pin + clear unseen
- write flush: compensation + `outputFollowEffects` → set unseen (~808–828)
- viewport probe → `pinnedToBottom = getViewportY() <= 0` (~955–956)
- UI: `{#if hasUnseenOutput}` New output button (~1019)

Pure helpers already in `terminalOutputPolicy.ts` lines 81–98.

## Code anchors

Behavior oracle tests in `TerminalRawView.test.ts` (must stay green):

- holds reading position when scrollback grows
- shows New output while scrolled away; click clears + snaps
- keyboard-open pins to bottom (existing)
- reconnect / fullscreen scrollback cases that touch pin state

## Test-first instructions

Extend `terminalOutputPolicy.test.ts` with `createScrollFollowPolicy` cases
(fail before API exists):

1. starts pinned, no unseen
2. `unpin()` → not pinned; `noteOutput()` → hasUnseen true; no snap
3. while pinned, `noteOutput()` → snapToBottom true, unseen stays false
4. `jumpToBottom()` → pinned, unseen false, snap requested
5. `setPinnedFromViewport(true)` clears unseen; `false` unpins
6. `pin()` / `resetOnReconnect()` → pinned, unseen false
7. `isPinned()` / `hasUnseen()` reflect state after each transition

API shape (exact names preferred):

```ts
export type ScrollFollowPolicy = {
  isPinned(): boolean;
  hasUnseen(): boolean;
  pin(): void;
  unpin(): void;
  jumpToBottom(): { snapToBottom: true };
  setPinnedFromViewport(atBottom: boolean): void;
  resetOnReconnect(): void;
  /** Apply output-follow for the current pin state; may set unseen. */
  noteOutput(): { snapToBottom: boolean; markUnseenOutput: boolean };
};

export function createScrollFollowPolicy(): ScrollFollowPolicy;
```

`noteOutput` must use existing `outputFollowEffects(isPinned())` and set
internal unseen when `markUnseenOutput` is true.

Focused red:

```bash
npm run web:test -- --run src/terminalOutputPolicy.test.ts
```

## Edit instructions

### A. Implement `createScrollFollowPolicy` in `terminalOutputPolicy.ts`

Internal: `pinned` (default true), `unseen` (default false).
Wire methods per API above. Do not change existing pure helpers’ signatures.

### B. Wire `TerminalRawView.svelte`

- Create `const scrollFollow = createScrollFollowPolicy()` in onMount.
- Replace direct `pinnedToBottom = …` / `hasUnseenOutput = …` with policy
  methods.
- Keep a reactive `hasUnseenOutput` $state (or equivalent) for the template:
  after each policy mutation that can change unseen, set
  `hasUnseenOutput = scrollFollow.hasUnseen()`.
- Write flush path:
  - if `scrollFollow.isPinned()` → write only
  - else → compensate with `scrollbackGrowthCompensation` as today
  - then `const follow = scrollFollow.noteOutput()`; snap or sync unseen UI
- Gesture scroll away → `scrollFollow.unpin()` + sync UI
- Viewport bottom probe → `scrollFollow.setPinnedFromViewport(...)` + sync
- jumpToBottom / keyboard pin / snapVisible / reconnect → policy pin/jump/reset
  + sync
- Local `let pinnedToBottom` should disappear; use `scrollFollow.isPinned()`
  at call sites (or a thin local mirror updated only via policy — prefer
  calling `isPinned()`).

### C. TERMINAL.md

If the scroll-follow row still says only “resize validity”, clarify:
`Scroll-follow state + resize validity | terminalOutputPolicy.ts`.

## Verification commands

```bash
npm run web:test -- --run src/terminalOutputPolicy.test.ts
npm run web:test -- --run src/components/TerminalRawView.test.ts
npm run web:check
npm run web:build
rg -n 'let pinnedToBottom' crates/ajax-web/web/src/components/TerminalRawView.svelte
# expect: no matches
```

## Acceptance criteria

- New scroll-follow unit tests green
- Existing TerminalRawView scroll/New-output/reconnect tests green
- No `let pinnedToBottom` in TerminalRawView
- Diff limited to Allowed files

## Stop conditions

- Need to change layout policy or zero-lag to make tests pass → stop
- Weakening TerminalRawView assertions → stop
- Scope creeps into paste/copy → stop
- Two failed delegate rounds → stop
