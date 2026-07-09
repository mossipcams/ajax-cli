# Plan: Terminal ownership contract (Term-1)

## Scope

Document and test-enforce Web terminal module ownership so patch culture
(new `*FlushPending` flags in `TerminalRawView.svelte`) is rejected by
contract. Docs + Vitest only.

## Non-goals

- Extracting/thinning `TerminalRawView` (Term-2)
- Engine swap, behavior fixes, CSS changes
- CONTRIBUTING.md (file may not exist; architecture.md link only)

## Approval

- Status: planning complete; implementation authorized by user
  (“plan both … using tdd imp packet”)
- Packet: `.planning/packets/term-1-01-terminal-ownership-contract.md`

## Delegation decision

`Delegation decision: delegated via model-router` (Cursor CLI worker).

## Task checklist

### Task 1: Term-1.01 — TERMINAL.md + ownership contract test

- [x] Test to write: `crates/ajax-web/web/src/terminalOwnership.test.ts`
- [x] Docs: `crates/ajax-web/web/TERMINAL.md` + `architecture.md` one-line link
- [x] Verify: `npm run web:test -- --run terminalOwnership.test.ts`
- [x] Packet path: `.planning/packets/term-1-01-terminal-ownership-contract.md`

## Validation ledger

- Planning: confirmed ~1532-line TerminalRawView, existing seams, no
  `web/TERMINAL.md` yet, no CONTRIBUTING.md in repo root
- Pre-impl: `npm run web:test -- --run terminalOwnership.test.ts` → FAIL
  (ENOENT TERMINAL.md; architecture pointer missing)
- Post-impl: same command → PASS (2 tests)
- Post-impl: `npm run web:check` → PASS (0 errors/warnings)

## Deviations

- Used `import.meta.dirname` (same as App.test.ts) instead of `fileURLToPath`
  because Vitest resolves `import.meta.url` as a non-file URL in this package.
- Ran `npm install` once (vitest missing from node_modules in this worktree).
