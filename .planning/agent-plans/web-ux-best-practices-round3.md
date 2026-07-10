# Web UX round 3: feedback, a11y, and orientation best practices

## Scope

Frontend-only polish batch across the cockpit shell (no terminal changes):

1. `ResultPanel.svelte`: error toasts announce assertively (`role="alert"`,
   `aria-live="assertive"`); success stays `role="status"` / polite.
2. `TaskList.svelte`: project pills show the server's per-repo
   `attention_items` count as a badge (no browser derivation) with an
   accessible label; active pill gets `aria-current="true"`.
3. `App.svelte`: bottom-nav Dashboard button gets `aria-current="page"` on
   dashboard/project routes; `document.title` follows the route
   ("web/fix-login — Ajax", "Settings — Ajax", "repo — Ajax", "Ajax").
4. `NewTaskSheet.svelte`: move focus onto the dialog when it opens (container
   focus, not an input — no iOS keyboard pop); `enterkeyhint="go"` on the
   title input.
5. `TaskList.svelte` empty state: orient + point at the New-task CTA
   ("All quiet — start a new task below." / "No tasks in {repo} yet — start
   one below.").
6. `types.ts`: `RepoSummary.attention_items?: number` (mirrors Rust
   `ReposResponse`; already serialized, fixture already carries it).

## Non-goals

- No TerminalRawView changes.
- No new inbox/severity visuals; no sort changes.
- No Rust/API changes (attention_items already ships).
- No focus-restore-on-close for the sheet (follow-up if wanted).

## Delegation decision

`Delegation decision: delegated via model-router` (user instruction:
"Delegate"). Packet: `.planning/packets/web-ux-a11y-round3.md`.

## Checklist

- [x] tdd-implementation-packet written (`.planning/packets/web-ux-a11y-round3.md`)
- [x] model-router lane selected (frontend → cursor-delegate / composer-2.5)
- [x] Delegate ran (2 rounds: behavior #1, then #2–#8)
- [x] Parent diff review against scope (snapshot-isolated; all edits in
      Allowed files; one existing selector updated without weakening its
      behavior assertion)
- [x] Failing-first tests present for each behavior (delegate report + spot
      verification)
- [x] Parent validation (all rerun by parent)
- [x] Accept / resume decision recorded — **Accepted** with parent fixups below

## Deviations

- Run 1: the packet heredoc failed to embed (cwd drift) — Cursor located and
  read the packet file in the repo on its own; implemented only behavior #1
  per the "exactly one behavior" wrapper. Run 2 dispatched with the packet
  embedded and explicit "#2–#8 complete this one bounded change".
- Delegate's "pre-existing" unhandled `ws does not work in the browser`
  rejection was NOT pre-existing. Parent root-caused it: `App.loadDetail`
  applied stale responses with no route guard, so a late detail response
  could mount a terminal between `unstubAllGlobals` and DOM cleanup (and, in
  production, briefly render the previous task's detail after a fast switch).
  Parent fixups: stale-response guard in `App.svelte` `loadDetail` (+
  red/green-proven regression test), module-scope WebSocket stub in
  `App.test.ts`, and a settle await in the resume test.

## Validation

- `npm run web:check` — 0 errors (164 files)
- `npm run web:test -- --run` — 34 files / 463 tests, **0 unhandled errors**
- Stale-guard regression test proven red with guard disabled, green with it
- `npm run web:build` — dist rebuilt
- `cargo test -p ajax-web install` — 7 passed
- `cargo nextest run -p ajax-cli -E 'test(web_backend)'` — 19 passed
- Not run: `npm run web:smoke` (Playwright) — recommend one visual pass on
  device before merge, same as rounds 1–2.
