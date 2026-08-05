# Docs Drift Sweep

## Scope

Reconcile user-facing and architecture docs with shipped reality: Safari-tab-first
Cockpit, optional Home Screen for Declarative Web Push only, no classic PWA
packaging / SW offline mutation.

## Non-goals

- GSD 9-doc regenerate
- `.planning/` packet rewrites
- Code or test changes

## Delegation decision

Delegation decision: not delegated because docs-only / non-code (AGENTS exception).

## Checklist

- [x] Create this ledger
- [x] Rewrite README Web Cockpit + daily loop; reconcile push
- [x] Update AGENTS.md + architecture.md guardrails
- [x] Fix web-cockpit.md + cockpit.md; light-skim siblings
- [x] Delete docs/react-migration-plan.md
- [x] Grep + claim spot-check; record results
- [x] Move historical `TERMINAL_{REBUILD_ACCEPTANCE,LEGACY_SURFACE_TESTS,BEHAVIOR_CONTRACT}.md` → `.planning/archive/terminal-rebuild/`; keep live `TERMINAL.md`

## Deviations

- Light touch on `docs/speech-input.md` Safari/PWA wording for contract alignment
  (sibling skim found it already mostly correct).
- Follow-up: move historical terminal rebuild planning docs out of
  `crates/ajax-web/web/` into `.planning/archive/terminal-rebuild/` (keep live
  `TERMINAL.md` ownership note in place).

## Validation

```text
rg 'Notifications are out of scope|does not support Web Push|no Home Screen PWA dependency|push, and Home Screen install surfaces are unsupported|installable PWA surface' README.md AGENTS.md architecture.md docs/ PRODUCT.md
→ no matches (exit 0)

README spot-check: Diff Review, Declarative Web Push, classic PWA packaging present
docs/react-migration-plan.md → deleted
cargo/npm → skipped (markdown-only)

Planning docs outside .planning/:
  moved TERMINAL_REBUILD_ACCEPTANCE.md, TERMINAL_LEGACY_SURFACE_TESTS.md,
  TERMINAL_BEHAVIOR_CONTRACT.md → .planning/archive/terminal-rebuild/
  remaining outside .planning/: durable docs only (README, architecture,
  AGENTS, PRODUCT, DESIGN, docs/architecture/*, docs/speech-input.md,
  crates/ajax-web/web/TERMINAL.md ownership note)
```
