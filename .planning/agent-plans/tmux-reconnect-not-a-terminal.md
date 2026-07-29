# Fix: reconnect `open terminal failed: not a terminal`

## Scope

- Fix Web Cockpit task-terminal reconnect after #692: setup must not use
  `tmux new-session -A` (attach-if-exists) from a non-TTY process.
- Keep #692 linger-on-disconnect + stable client-id reconnect behavior.
- Correct the architecture.md claim that setup uses `new-session -A`.

## Non-goals

- Reaper / destroy path changes
- Client-id hashing / query parsing
- Browser reconnect backoff / seed=0 behavior
- Broader tmux attach PTY changes

## Root cause

`run_tmux_command_blocking` runs setup via `Command::output()` with no PTY.
`new-session -Ad` works on first create (`-d` = detached). On reconnect the
ephemeral session already exists, so `-A` becomes `attach-session`, which
needs a terminal → stderr `open terminal failed: not a terminal` → bridge
reports `failed to create terminal session: …`.

Reproduced locally with tmux 3.6a.

## Delegation decision

`Delegation decision: delegated via model-router`

## Task checklist

- [x] Packet READY; `scripts/check-packet` passes
- [x] Failing test: setup uses `-d` (not `-Ad`); duplicate-session setup
      failure is ignored; real failures are not
- [x] Implement: `-Ad` → `-d`; ignore `duplicate session` on `new-session`
      setup only
- [x] Update `architecture.md` setup idempotency wording
- [x] Validation: focused ajax-web terminal_pty tests green
- [x] Parent review of delegate diff

## Validation

```bash
cargo test -p ajax-web --lib adapters::terminal_pty
cargo check -p ajax-web
```

## Deviations

- pi/GLM and pi/MiniMax both hit opencode-go monthly usage limit (429).
- Rerouted to cursor-delegate / composer-2.5. Runner reported
  `MISSING_STRUCTURED_REPORT` but produced a correct in-scope diff; parent
  reviewed and validated personally.

## Results

- `cargo test -p ajax-web --lib adapters::terminal_pty` → 33 passed
- `cargo check -p ajax-web` → ok
- Live tmux smoke: first `-d` create ok; reconnect `duplicate session` (ignored
  by helper); set-option on existing ephemeral ok
