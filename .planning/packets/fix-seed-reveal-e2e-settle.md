PACKET_STATUS: READY
TASK_KIND: tests-only
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Task

Fix Web CI smoke flakes on PR #732: after restoring `SEED_REVEAL_QUIET_MS=120`,
seeded-open quiet reveal can re-pin `followLive` before New-output / viewport
scroll e2e scroll away. Wait for seed reveal to settle after the first
scrollback emit, then scroll / assert New output.

## Scope

### Allowed

- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `.planning/agent-plans/fix-seed-reveal-load-scroll.md` (checklist / deviations only)

### Forbidden

- Do not commit, push, merge, rebase, or change branches.
- Do not edit `TaskTerminal.tsx`, `TaskTerminal.test.tsx`, `dist/*`, or CSS.
- Do not change production seed-reveal timing or scroll-sync behavior.
- Do not edit files outside Allowed.

## Acceptance

1. Add helper near `scrollInteractionSurfaceAway`:

```ts
async function waitForSeedRevealSettled(page: import("@playwright/test").Page) {
  await expect(terminalInteractionSurface(page)).not.toHaveClass(/is-seed-pending/, {
    timeout: 5_000,
  });
}
```

2. Call `await waitForSeedRevealSettled(page);` **after** the first
   `emitLatestTerminalOutput(..., scrollbackChunk(...))` and **before**
   `scrollInteractionSurfaceAway` in these tests:
   - `terminal controls meet mobile touch target size on phone`
   - `scrolling the interaction wrapper moves the terminal viewport`
   - `reading scrollback shows New output and restoring live output sends no PTY input`
   - `New output click does not refocus xterm or reopen keyboard, and direct surface click focuses without scrolling`

3. In `scrolling the interaction wrapper moves the terminal viewport`, replace
   the immediate `expect(atBottom).toBeGreaterThan(0)` with
   `await expect.poll(async () => viewportY()).toBeGreaterThan(0);` then
   `const atBottom = await viewportY();`.

4. Do not change the seeded-open quiet e2e (80ms mid-gap) or production source.

## Constraints

- Smallest diff; helper + four call sites (+ poll tweak).
- Keep existing New output / viewport assertions after the wait.

## Code anchors

- Helper placement: after `scrollInteractionSurfaceAway` (~131–137).
- Touch-target test: emit 200 rows then scroll (~1638).
- Viewport scroll test (~2374).
- Reading scrollback New output (~2397).
- New output click (~2417).

## Verification

```yaml
verification:
  methods:
    - type: other
      command: rg -n 'waitForSeedRevealSettled' crates/ajax-web/web/e2e/terminal-behavior.test.ts
      expected: helper defined once; called in the four tests above
    - type: existing_test
      command: NONE
      steps:
        - Parent will push and rely on CI Web smoke; local playwright needs web:smoke harness
      expected: diff matches acceptance; no production edits
  broader_checks: []
  reason: Tests-only e2e timing fix; CI Web job is the behavioral proof.
```

## Stop if

- Fix seems to need production TaskTerminal changes again.
- Diff grows beyond the e2e helper + four sites.
- Seeded-open quiet e2e assertions are weakened.
