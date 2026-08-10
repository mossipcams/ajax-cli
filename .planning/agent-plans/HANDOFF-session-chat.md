# Handoff — ajax web orchestration session

Written 2026-08-10. Branch `ajax/feature-flags-for-ajax-web-session`,
worktree `ajax-cli__worktrees/ajax-feature-flags-for-ajax-web-session`.
Tree clean, **nothing pushed**, six commits from the last session.

## What happened

Matt: "Redesign ajax web chat ux and flow its awful right now to the point
where its unusable" → "use impeccable for the full redesign". It was
genuinely unusable, and the causes were structural rather than cosmetic.

| Commit | What |
| --- | --- |
| `b0345590` | Redesign: live head over a settled transcript |
| `be9c6572` | `mcpServers: []` — **the session had never once worked** |
| `40a018ee` | Polish: reasoning tail, composer auto-grow, focus states |
| `3eef9eb4` | Audit fixes: WCAG AA, 44px targets, reduced motion, memoised rows |
| `d1ec0da2` | Transcript log on the host, cursor per socket |
| `3d9f3488` | Bottom-anchored short transcripts |

**The design.** The session is an instrument, not a message list: a fixed live
head reports the running tool, its file, and any blocking decision; a
transcript below receives only *settled* work, so it holds scroll position
while the agent streams. Full direction contract is the header comment of
`features/session/SessionChat.tsx` — read it before changing that surface.

**The bug that mattered most.** Cursor's ACP rejects `session/new` without
`mcpServers` as an array, reporting it only as JSON-RPC `"Internal error"`.
Confirmed against the real `agent` binary. Anything Matt tested before
`be9c6572` was failing for that reason.

## Open decisions — Matt's, not yours

1. **The shell.** His words: "ajax web design is extremely dumb ... the spacing
   and purpose is off." He is right, and it is the biggest remaining win.
   Diagnosis is already done, see below. Untouched.
2. **Transcript durability across a server restart.** Needs a store.
   `registry_events` has exactly the right shape (`sequence`, `task_id`,
   `kind`, `message`, `occurred_at`) but putting chat there puts conversation
   inside the registry, and AGENTS.md says core owns *task truth*. Boundary
   call.
3. **`DESIGN.md` §5 scoped exception** (committed in `b0345590`) recording that
   this route ships a composer where the doc says a terminal belongs. Reverts
   cleanly if unwanted. It also left `.impeccable/design.json` stale —
   `/impeccable document` refreshes it.
4. **Two hook waivers**, both verified false positives, neither suppressed:
   - `bounce-easing` on `var(--ease-spring)` = `cubic-bezier(0.22, 1, 0.36, 1)`,
     easeOutQuint. No control point exceeds y=1, so no overshoot.
   - `design-system-color` on `#000` in `styles.css` — the opaque stop of a
     `mask-image`, consumed as alpha. Never painted.

## The shell work, pre-diagnosed

Evidence: screenshots in the last session, plus source. **Not a spacing
problem — padding it would be the exact failure `layout.md` names.** The space
has no job.

- Task detail stacks **four chrome bars** before content (global header, task
  header, interact strip, then the terminal). Roughly **half the viewport is
  chrome**, on the surface DESIGN.md says chrome must never compete with.
- **Status is stated three times** there; the dashboard states its count three
  ways (`1 running`, `ACTIVE 2`, per-row `RUNNING`).
- The bottom bar spends **half its width on "Dashboard" while you are on the
  dashboard** — a no-op control on every screen, duplicating BACK two bars up.
- ~55% of the dashboard is void with two tasks.

Route it as `/impeccable critique` then `distill`. It touches every screen, so
give it a full context budget.

⚠ A dead-space probe written last session **mis-measured the dashboard** (it
counted the nav bar's own label as content and reported 0% void). Do not trust
that number; re-measure or use screenshots.

## Traps this session cost real time on

- **`rtk` rewrites stdout.** `git diff`, `grep`, and `python` prints come back
  mangled or empty — it once made a live CSS edit look like it had vanished.
  Write output to a file in the scratchpad and `Read` it.
- **Dev web is HTTPS on :8788**; plain-HTTP probes return garbage that looks
  like a sandbox block. Stable is :8787. `.ajax-dev-web/bin` is shared across
  worktrees — marker-grep the binary before trusting a device test.
- **`--ink-faint` is CLI-locked** (`#808080`, xterm 244, lockstep with
  ajax-tui). It measures 4.27:1 and fails AA, but **do not change the token** —
  promote the specific reading-text declarations to `--ink-muted` instead.
- **`[data-outlet]` sections in `App.tsx` are `display:block`** and silently
  break any full-height flex route. Already in memory as
  `route_outlet_breaks_flex_chain`.
- **The husky pre-commit hook** runs `web:build` + full `verify` + a release
  build + `cargo install`. Minutes. Background it; never `--no-verify`.
- **`npm run verify` does not include Playwright.** Run `pkill -f vite` first,
  then `CI=1 npm run web:smoke`, or a stale server false-passes.
- **The smoke suite has pre-existing flakes** in `terminal-behavior.test.ts` /
  `actions.test.ts` — 1–2 per run, different each time, each passing in
  isolation. Verified by stashing the whole change and reproducing on the base.
  Not caused by this work.

## State of the gates

263 web tests in the session suites, 763 total web, 275 `ajax-web` Rust, 18/18
session e2e on desktop-chromium and mobile-webkit, clippy `-D warnings`, fmt,
lint, ast-grep, `verify:arch`, `ci:verify` all green at `3d9f3488`.

Plans: `session-chat-redesign.md`, `session-transcript-log.md` in this
directory carry the full defect lists, deviations, and verdicts.
