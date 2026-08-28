# Ajax Web — runtime control panel

Mobile-first operator surface for the currently installed Ajax web control
plane. Separate from Settings **Test in Stable** and task-details **Test in
Dev**, which remain development testing workflows.

Approval: requested immediate implementation (user: Build).
Status: complete.

## Scope

- New `#/control` operator page inside Ajax Web (not a Settings rewrite).
- Current server status: health, version, commit, profile, uptime, update
  availability.
- **Restart Ajax**: restart only the currently installed web control plane.
- **Update Ajax**: deploy `origin/main` using existing safe-deploy mechanics.
- Live progress, recent lifecycle logs, success / failure / automatic rollback.
- Reconnect UX while the listener is down; durable operation state so the
  browser can show the real result after cutover, not only a timeout.
- Focused tests and `docs/architecture/web-cockpit.md` ownership update.

## Non-goals

- Do not rename, remove, relocate, or change Test in Stable / Test in Dev
  button behavior, routes, or copy.
- Do not add OliveTin, Docker, launchd, or another daemon.
- Do not add arbitrary shell-command APIs or broad process matching.
- Do not touch task tmux sessions, worktrees, branches, agent processes, or
  task records.
- Do not make the browser a second process-management or registry authority.

## Existing architecture to reuse

- `adapters::server` + `POST /api/server/restart` (today: re-exec or spawn
  `AJAX_WEB_RESTART_SCRIPT`, which is `dev-web-restart.sh` and **rebuilds**
  when that env is set). Settings does not call this.
- Settings Test in Stable: `POST /api/server/test-in-stable` →
  `scripts/test-in-stable.sh` (setsid + tmux `ajax-test-in-stable`) →
  `dev-web-restart.sh --profile stable`. Live listener stays up during build.
- Test in Dev: `slices::dev_deploy` + `GET/POST /api/dev-deploy`. Untouched.
- `dev-web-restart.sh`: isolated main worktree, build before cutover, exact
  tmux `ajax-web-<profile>`, health curl, restore previous cargo/slot binary.
- Browser reconnect: `waitForServerRestart` + public `GET /api/health`.
- `/api/version`: `version`, `profile`, `test_in_stable`. Extend, do not
  replace. Keep `/api/health` a public reachability probe (`{ok:true}`).

## Design

### Ownership

New `ajax-web::slices::runtime_control` owns durable operation records, status
projection, and admission. It does not own Git/tmux/cargo; it launches the
existing scripts with explicit flags. `adapters::server` stays the process
launch adapter. Core/registry/task slices stay out of this path.

Do not grow `runtime/mod.rs` or `adapters/server.rs` past the file-size limit;
peel handlers into the new slice.

### HTTP (browser-session cookie, same policy as `/api/version`)

| Method | Path | Role |
| --- | --- | --- |
| GET | `/api/server/runtime` | Status + current/last operation + recent logs + update availability |
| POST | `/api/server/restart` | Restart-only. Must not fetch, build, install, or update. |
| POST | `/api/server/update` | Update from origin/main using Test in Stable mechanics |
| POST | `/api/server/test-in-stable` | Unchanged development workflow |

`GET /api/health` stays public and dumb. Lifecycle truth lives on
`/api/server/runtime`.

### Durable operation state

Write `<host-clone>/.ajax-dev-web/runtime-control.json` (and a bounded JSONL
log beside it) **before** the listener dies. The restart/update script updates
phases. The new process reads the file on boot. The browser polls
`GET /api/server/runtime` after reconnect and shows succeeded / failed /
rolled_back instead of inventing a timeout when health returns.

Phases: `queued` → (`fetching`/`building`/`installing` for update only) →
`restarting` → `health_check` → `succeeded` | `failed` | `rolled_back`.

### Restart

Detach like Test in Stable (setsid + tmux session that
`stop_tmux_session` does not kill, e.g. `ajax-runtime-restart`). Invoke
`dev-web-restart.sh --restart-only --profile <current> --port <current>`.

`--restart-only` must:

- skip fetch, worktree reset, npm, cargo install, hook reinstall;
- start the currently installed binary for that profile (cargo bin for
  stable, slot bin for dev if that is what is running);
- target exact tmux session `ajax-web-<profile>` only;
- refuse unmanaged listeners (existing `stop_listener` behavior);
- write progress to the durable file.

Change `POST /api/server/restart` from “spawn full rebuild script when
`AJAX_WEB_RESTART_SCRIPT` is set” to this restart-only path. Document it.
Keep cfg(test) as a no-op that still returns `{ok:true,restarting:true}` plus
a durable queued/restarting record when tests exercise the slice.

### Update

Reuse Test in Stable mechanics: isolated main worktree, build before cutover,
exact `ajax-web-stable` targeting, health validation, restore previous
`~/.cargo/bin/ajax-cli` on failure. Spawn via the existing detached wrapper
pattern (do not make the live server a child of `kill-session`).

Update always deploys **stable** from `origin/main` (same as Test in Stable).
It must not install origin/main into the Test in Dev slot binary.

Refuse when `ajax-test-in-stable` or an in-flight runtime update session
exists (one cargo install at a time). Optional `AJAX_RUNTIME_STATUS_FILE`
lets `dev-web-restart.sh` write structured phases without changing Test in
Stable when the env is unset.

Rollback: existing `restore_previous_binary` / `restore_previous_cargo_bin`;
record `rolled_back` in the durable file when the previous artifact is
running after a failed new start or failed health check.

### Status fields

- `ok` / listener health
- `version` (existing `install::app_version()`)
- `commit` (last successful deploy SHA persisted in the durable file; unknown
  is allowed)
- `profile`
- `uptime_seconds` (this process start)
- `update_available` (`git ls-remote origin refs/heads/main` vs installed
  commit; no worktree mutation; unknown on failure)
- `operation` (kind, phase, started_at, finished_at, result, rollback)
- `logs` (recent lifecycle lines, redacted and size-bounded)

### Frontend

New feature `features/runtime-control/` with `public.ts`. Route `#/control`.
Bottom-nav destination alongside Dashboard / New. Settings header link and
Test in Stable stay put.

Reuse the React design system (`Button`, Settings typography/spacing tokens,
44px targets, `env(safe-area-inset-*)`). iPhone Safari: reconnect overlay
while health is down; after health returns, poll runtime status and show the
durable result. Two-tap confirm on Restart and Update.

Do not persist operational data in IndexedDB. Transient “operation in flight”
in memory plus server durable state is enough.

### Safety

- Exact tmux names only: `ajax-web-<profile>`, plus the detached launcher
  session. Never `tmux ls` + grep for tasks.
- Never kill, list, or rewrite task worktrees/branches/registry rows.
- No public unauthenticated mutation. Runtime GET/POST require the browser
  session cookie.
- Log redaction: tokens, cookies, `Authorization`, private keys.

## Tasks

- [x] T1 — `dev-web-restart.sh --restart-only` plus detached restart wrapper;
      structured status file for update/restart; rollback writes `rolled_back`.
- [x] T2 — `slices::runtime_control` + GET/POST routes; durable JSON; uptime;
      update availability; log tail + redaction.
- [x] T3 — Make `POST /api/server/restart` restart-only; add
      `POST /api/server/update`; leave Test in Stable/Dev unchanged.
- [x] T4 — `#/control` page, bottom nav, reconnect + progress UI, iPhone
      Safari layout.
- [x] T5 — Focused tests: restart-only (no fetch/build/install), update
      progress, reconnect reads durable result, rollback, log safety, active
      task tmux/worktrees untouched.
- [x] T6 — `docs/architecture/web-cockpit.md` runtime control panel section;
      architecture slice list; CSS/ESLint feature boundaries.

## Verification

```bash
# script / slice tests covering --restart-only, status file, rollback, log redaction
cargo nextest run -p ajax-web runtime_control server
# plus any new bash tests next to the script if the repo pattern exists

cd crates/ajax-web/web
npx vitest run src/features/runtime-control src/shared/lib/api.test.ts src/shared/lib/routes.test.ts
npm run web:check
npm run web:lint
```

Architecture tests that enumerate slices / stylesheet inventory must be
updated in the same change. Do not claim full `npm run verify` unless it ran.

## Deviations

Parent review follow-up (live progress polling, dev-profile update
`restarting:false`, gated JSONL logs, serde error bodies, POST failure overlay,
focused tests, docs alignment) — complete.

Ship blockers follow-up:

- [x] Full-screen reconnect overlay only when `restarting:true`; live status/logs
      stay visible during non-restarting update progress.
- [x] POST failures use dismissible in-page error; Back, nav, and actions stay
      usable (no “Waiting for the listener to return…” on failed POST).
- [x] Rebuilt `crates/ajax-web/web/dist` via `npm run web:build`.
- [x] Cleared `runtime_control` slice clippy dead_code warnings.
