# TDD Implementation Packet — Term-1.01: Terminal ownership contract

## 1. Goal

Add an enforceable ownership contract for the Web Cockpit terminal shell so
future fixes land in the correct module — not as new one-shot flags in
`TerminalRawView.svelte`. This packet is **docs + contract test only** (no
runtime behavior change, no extract/refactor of TerminalRawView).

## 2. Allowed files

Test files:

- `crates/ajax-web/web/src/terminalOwnership.test.ts` (**create**)

Production / docs files:

- `crates/ajax-web/web/TERMINAL.md` (**create**)
- `architecture.md` (one short pointer under the Web terminal frontend
  paragraph only)

Planning ledger:

- `.planning/agent-plans/term-ownership-contract.md`
- `.planning/packets/term-1-01-terminal-ownership-contract.md` (this file)

## 3. Forbidden changes

- Do not edit `TerminalRawView.svelte`, `terminal*.ts`, `viewport.ts`, CSS,
  Svelte components, Playwright e2e, or Rust runtime.
- Do not extract/refactor terminal code (that is Term-2).
- Do not swap Ghostty / change dependencies / touch `dist/`.
- Do not add CONTRIBUTING.md if it does not exist (architecture.md link is
  enough for this packet).
- Do not weaken existing tests.
- Do not commit, push, or change branches.

## 4. Architecture context

From `architecture.md` (`ajax-web::slices::terminal` / browser frontend):

- Raw Ghostty/tmux-first; no Live/snapshot/composer default
- Frontend modules already named: `terminalConnection.ts`,
  `terminalGestures.ts`, `terminalGeometry.ts`, `terminalRefit.ts`,
  `viewport.ts`, `TerminalRawView.svelte`
- These modules must not own task truth or tmux target selection

vTerm-1 goal: make that ownership **explicit and test-enforced** so patch
culture (`*FlushPending` in the Svelte file) stops being the default.

## 5. Code anchors

Current god-component evidence (cite in TERMINAL.md anti-patterns; do not edit):

```text
crates/ajax-web/web/src/components/TerminalRawView.svelte
  ~1532 lines
  let pinchFlushPending = false;
  let expandFlushPending = false;
```

Existing seams to list as owners:

```text
crates/ajax-web/web/src/viewport.ts              — keyboard / visualViewport
crates/ajax-web/web/src/terminalGeometry.ts      — fit / font / pan math
crates/ajax-web/web/src/terminalRefit.ts         — refit scheduling
crates/ajax-web/web/src/terminalGestures.ts      — gestures / selection geometry
crates/ajax-web/web/src/terminalOutputPolicy.ts  — scroll-follow / size validity
crates/ajax-web/web/src/terminalConnection.ts    — WS lifecycle / backoff
crates/ajax-web/web/src/components/TerminalRawView.svelte — mount + chrome only
```

architecture.md insert point:

```text
architecture.md
  ### `ajax-web::slices::terminal`
  paragraph beginning: "The browser terminal frontend lives in `crates/ajax-web/web`."
  → after that paragraph, add one sentence + link to TERMINAL.md
```

Existing test style to mirror (Vitest file next to source):

```text
crates/ajax-web/web/src/viewport.test.ts
crates/ajax-web/web/src/terminalOutputPolicy.test.ts
```

## 6. Test-first instructions

Create `crates/ajax-web/web/src/terminalOwnership.test.ts`.

**Test name:** `TERMINAL.md documents ownership and anti-patterns`

**Behavior:**

1. Resolve `TERMINAL.md` relative to the web package root
   (`import.meta.url` → `../TERMINAL.md` or `path.join` from file URL — match
   how other web tests read files if any; otherwise use
   `readFileSync` from `node:fs` + `fileURLToPath`).
2. Assert file exists / readable.
3. Assert body includes these substrings (exact enough to lock the contract):

| Must appear | Why |
| --- | --- |
| `viewport.ts` | keyboard owner |
| `terminalGeometry.ts` | geometry owner |
| `terminalRefit.ts` | refit owner |
| `terminalGestures.ts` | gesture owner |
| `terminalOutputPolicy.ts` | output policy owner |
| `terminalConnection.ts` | connection owner |
| `TerminalRawView.svelte` | orchestration owner |
| `FlushPending` or `one-shot` | anti-pattern named |
| `failing test` or `Playwright` | review rule: test first |
| `Live/snapshot/composer` or `Live` | non-goal locked |

4. Optional second test: `architecture.md points at TERMINAL.md` — read
   repo-root `architecture.md` and assert it contains
   `crates/ajax-web/web/TERMINAL.md` or `` `TERMINAL.md` ``. If pathing from
   web tests to repo root is awkward, skip this second test and only add the
   architecture sentence manually; the TERMINAL.md content test is required.

### Pre-impl verification (must fail)

```bash
cd crates/ajax-web/web && npm run web:test -- --run terminalOwnership.test.ts
```

Expected: FAIL — file missing or assertions fail.

## 7. Production edit instructions

1. Create `crates/ajax-web/web/TERMINAL.md` with at least:

```markdown
# Web Cockpit terminal ownership

## Product contract
- Raw Ghostty/tmux-first on mobile and desktop
- Do not reintroduce Live/snapshot/composer as default
- Browser modules do not own task truth or tmux target selection

## Ownership table
| Concern | Owner |
| Keyboard / visualViewport / --app-* | viewport.ts |
| Fit / font / pan math | terminalGeometry.ts |
| Refit scheduling | terminalRefit.ts |
| Gestures / selection geometry | terminalGestures.ts |
| Scroll-follow / resize validity | terminalOutputPolicy.ts |
| WS connect / backoff / status | terminalConnection.ts |
| Ghostty mount + chrome UI | TerminalRawView.svelte |
| Route scroll / chrome hide | styles.css + App layout |

## Anti-patterns
- Do not add new one-shot `*FlushPending` (or equivalent) booleans in
  TerminalRawView.svelte — put named policy in terminalRefit.ts / geometry
- Do not fix iOS bugs only in CSS/component without a failing Vitest or
  mobile-webkit Playwright case first
- Do not scatter Ghostty private API casts; isolate in one adapter when extracting

## Review rule
Terminal behavior PRs: failing test first; policy change in the owning module.
```

   Keep it short. No novel architecture beyond the table above.

2. In `architecture.md`, in the browser terminal frontend paragraph under
   `### ajax-web::slices::terminal`, add one sentence:

   `Frontend ownership rules for these modules are in \`crates/ajax-web/web/TERMINAL.md\`.`

3. Make the Vitest pass. No other edits.

## 8. Verification commands

```bash
# Pre-impl (expect FAIL)
cd crates/ajax-web/web && npm run web:test -- --run terminalOwnership.test.ts

# Post-impl
cd crates/ajax-web/web && npm run web:test -- --run terminalOwnership.test.ts
cd crates/ajax-web/web && npm run web:check
```

## 9. Acceptance criteria

- [ ] `terminalOwnership.test.ts` failed before `TERMINAL.md` existed
- [ ] Test passes after docs added
- [ ] `TERMINAL.md` lists all seven owners + anti-patterns + review rule
- [ ] `architecture.md` links to `TERMINAL.md`
- [ ] No runtime/Svelte/terminal*.ts behavior changes in the diff
- [ ] Diff limited to allowed files

## 10. Stop conditions

Stop and ask the parent if:

- Vitest cannot read files from disk in this package (then ask for alternate
  contract-test location under `ajax-web` Rust tests)
- You think you need to edit `TerminalRawView.svelte` to “make the test pass”
- Pre-impl test passes without `TERMINAL.md`
- Scope creeps into Term-2 extraction

## Delegation

`Delegation decision: delegated via model-router` after packet approval.

Suggested lane: **Cursor Composer 2.5** (docs + small Vitest). Parent reviews
diff before accept.
