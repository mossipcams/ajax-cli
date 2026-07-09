# TDD Packet: Mobile scrollback + browser tmux redraw noise (PR7)

## 1. Goal

Use 2000-line scrollback on mobile and 10000 on desktop for Ghostty, and apply browser-ephemeral-session-only tmux options that reduce status redraw noise without touching the shared task session.

## 2. Allowed files

- `crates/ajax-web/web/src/terminalGeometry.ts` (or `terminalOutputPolicy.ts` for constants + mobile detect helper)
- matching `*.test.ts`
- `crates/ajax-web/web/src/components/TerminalRawView.svelte`
- `crates/ajax-web/web/src/components/TerminalRawView.test.ts` (only if needed)
- `crates/ajax-web/src/adapters/terminal_pty.rs`
- `.planning/agent-plans/web-mobile-power-optimizations.md` (optional)

## 3. Forbidden changes

- Do not change shared (non-ephemeral) tmux session options.
- Do not disable scrollback entirely.
- Do not change attach/teardown kill-session semantics beyond adding set-option setup commands for the ephemeral session.
- No binary/protocol changes (PR6). No polling changes.

## 4. Architecture context

Ephemeral grouped sessions (`new-session -t <shared>`) isolate phone resize; they can also carry quieter status settings. Ghostty accepts `scrollback` / `scrollbackLimit` in constructor options (default ~10000 in wasm).

## 5. Code anchors

```ts
// TerminalRawView.svelte mount
term = new Terminal({
  ...
  // add scrollback: terminalScrollbackLines(),
});
```

Ghostty types: `scrollback?: number` and/or `scrollbackLimit?: number` — use whichever the Terminal constructor options accept (check `node_modules/ghostty-web/dist/index.d.ts` `ITerminalOptions`). Prefer the option that actually limits buffer size.

```rust
// terminal_pty.rs build_isolated_attach_plan_with_token
setup: vec![TmuxCommand::new(["new-session", "-d", "-s", &ephemeral, "-t", &plan.tmux_session])],
```

Extend `setup` with ephemeral-only:

```text
tmux set-option -t "$EPHEMERAL" status-interval 5
tmux set-option -t "$EPHEMERAL" visual-activity off
tmux set-option -t "$EPHEMERAL" visual-bell off
```

## 6. Test-first instructions

**TS:**

1. `MOBILE_SCROLLBACK_LINES === 2000`
2. `DESKTOP_SCROLLBACK_LINES === 10000`
3. `terminalScrollbackLines()` returns mobile when coarse/narrow heuristic matches; desktop otherwise
   - Reuse existing mobile media heuristic if one exists; else:
     `matchMedia("(max-width: 767px), (pointer: coarse) and (max-height: 500px)")` matching CSS in TaskDetail.

**Rust:**

1. Update `isolated_attach_plan_creates_grouped_session_then_attaches` (or add sibling test) to assert setup includes the three `set-option` commands targeting the ephemeral session name after `new-session`.
2. Assert shared session name never appears as `-t` target of those set-options (only ephemeral).

**Fail first**, then implement.

```bash
npm run web:test -- crates/ajax-web/web/src/terminalGeometry.test.ts --run
# or wherever constants live
cargo test -p ajax-web isolated_attach -- --nocapture
```

## 7. Production edit instructions

1. Add constants + `terminalScrollbackLines()` helper.
2. Pass into `new Terminal({ scrollback: ... })` (or correct option name).
3. Extend `IsolatedAttachPlan.setup` after `new-session` with three `set-option -t <ephemeral> ...` commands.
4. Keep teardown as kill-session only.

## 8. Verification commands

```bash
npm run web:test -- crates/ajax-web/web/src/terminalGeometry.test.ts crates/ajax-web/web/src/components/TerminalRawView.test.ts --run
cargo test -p ajax-web isolated_attach reaper_targets filter_scrollback -- --nocapture
```

## 9. Acceptance criteria

- Mobile 2k / desktop 10k scrollback constants and selection helper tested.
- Terminal constructed with the selected limit.
- Ephemeral setup sets status-interval 5, visual-activity off, visual-bell off on ephemeral only.
- Shared session options untouched.

## 10. Stop conditions

- Ghostty option name unclear after checking d.ts — stop and report both candidates.
- set-option must run after attach rather than in setup and existing flow cannot support it without large rewrite — stop and report.
