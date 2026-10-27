# Web Cockpit supervision: tmux → launchd

Approval: user requested immediate implementation (2026-10-27).

## Scope

- `ajax web service install|uninstall|start|stop|status` (macOS launchd user
  agent, `gui/<uid>` domain). Plist: RunAtLoad (start at login), KeepAlive
  `SuccessfulExit=false` (restart on failure), per-profile label
  `com.ajax.web.<profile>`, logs under `~/Library/Logs/`, PATH captured from the
  installing shell, `AJAX_WEB_RESTART_*` forwarded when set.
- `scripts/dev-web-restart.sh` background mode supervises via
  `ajax web service install` + start instead of a tmux session.
- Chat task provisioning: verify (regression tests) that orchestration-chat
  tasks issue no tmux commands at create/start/open/refresh and work when the
  `tmux` binary is absent; fix any chat-path tmux dependency found.
- Docs: `docs/architecture/web-cockpit.md`, README.

## Non-goals

- No change to terminal tasks (tmux remains their substrate).
- No change to SQLite schema, chat persistence, recovery, or lifecycle states.
- No Linux/systemd support. Docker compose path untouched.

## Facts from discovery

- Chat tasks already skip tmux at plan time:
  `crates/ajax-core/src/commands/new_task.rs:175` (`skip_interactive_agent` +
  ACP-capable harness). Remaining tmux touchpoints on chat tasks are explicit
  Terminal attach / promote-on-attach and refresh probes (tolerated).
- Prior launchd art: commit `52ebd532` (`crates/ajax-cli/src/server.rs`,
  removed in #54).

## Tasks

- [x] A. launchd service module + CLI wiring + tests (ajax-cli)
- [x] B. server self-restart launchd-aware; dev-web-restart.sh → launchd
- [x] C. chat-without-tmux regression tests (passed on existing code; no fix needed)
- [x] D. docs (web-cockpit.md, README)
- [x] Parent review of each delta; gate run (clippy all-targets, nextest)
- [ ] Live launchd cycle on a real login session (blocked in agent sandbox)

## Deviations

- KeepAlive is `true` (not `SuccessfulExit=false`): the server's self-restart
  exits 0, so restart-on-failure-only would leave the service down.
  `AbandonProcessGroup=true` keeps the spawned restart script alive.
- Server self-restart under `AJAX_WEB_SUPERVISOR=launchd` without a restart
  script now exits and lets launchd respawn (no unsupervised replacement).
- Browser-disconnect durability: covered by existing tests
  (`web_session` replay/restore/`queued_prompts_continue_after_operator_disconnects`);
  no new test added.

## Validation

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: pass.
- `env -u RUST_LOG cargo nextest run --workspace`: 3250/3254 pass; 4 timing
  tests (stt_provider, ajax-supervisor x2, live_cli) failed under load avg ~98
  and pass on isolated reruns (3x).
- `node --test scripts/dev-web-restart.test.mjs`: 11/11. `bash -n`: ok.
- Live `ajax web service install`: `plutil -lint` OK, but `launchctl
  bootstrap` fails with EIO for ANY plist (even /bin/sleep) inside the nono
  sandbox over SSH, so the real launchd cycle is unverified.
