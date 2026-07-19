# Slice 12 — Remaining audit findings

Parent: `react-migration-cleanup.md`
Branch tip at start: `fb11179` (slice 11 `terminal.js` landed)

## Decision

Clear the deferred ESLint backlog marked `// slice 12 follow-up` in
`crates/ajax-web/web/eslint.config.mjs`, then turn those rules to `error`.
No permanent testing-library exemptions — use accessible queries or explicit
`data-testid` hooks. No product behavior change beyond those hooks + a11y
disables that document intentional backdrop dismiss.

## Scope

- Recount and fix every rule still `off` with a `slice 12 follow-up` marker
- Refresh stale path comments (pre–slice 9 `src/components/…` → current paths)
- **No** permanent testing-library exemptions
- Update master plan / handoff when the backlog is empty

## Non-goals

- No TaskTerminal controller extraction (slice 10 leftovers stay closed)
- No new code splits, shadcn primitives, or feature work
- No weakening tests to silence lint
- No type-aware ESLint / `parserOptions.project`

## Waves (done)

- **A** — mechanical + already-clean rules → `error`
- **B** — testing-library + NewTaskSheet a11y; exemptions removed
- **C** — close-out verify

## Method

`Delegation decision: not delegated because` `scripts/run-delegate` is missing
from this worktree. Parent implemented Waves A–C locally.

## Checklist

- [x] Inventory + recount
- [x] Wave A green; rules enabled
- [x] Wave B green; rules enabled — **no permanent TL exemptions**
- [x] Wave C close-out + verify
- [x] Commit / push (on request)

## Deviations

- `prefer-const` for `fitAddon` / `refitController`: deferred `let` + disable
- `no-control-regex`: disable-next-line on CSI ESC pattern
- `vitest/expect-expect`: allow `expectHeightBandPin`
- Matt: no permanent TL exemptions — testids + accessible queries instead
- NewTaskSheet form `aria-label="New task"`; backdrop click disable-next-line

## Validation (Wave C)

```text
npm run web:sg                         OK
npm run web:build:check                OK (app.js + terminal.js)
npm run verify                         OK (fmt/check/clippy/nextest/doc/web)
cargo build --release -p ajax-cli      OK
cargo install --path crates/ajax-cli   OK
cargo nextest run -p ajax-web          OK (included in verify)
```

Slice 12 complete pending commit.

