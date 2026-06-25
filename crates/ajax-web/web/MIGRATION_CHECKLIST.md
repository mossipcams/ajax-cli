# Ajax Web Frontend Migration — Behavior Inventory & Parity Checklist

This checklist freezes the **current** browser behavior of the legacy
`index.html` + `app.js` + `app.css` shell so the Svelte + TypeScript migration
can be verified for parity. Every item below is a behavior that exists today and
must continue to work after each migration phase. Source references are to the
legacy files as of the start of the migration.

The browser is a **rendering projection only**. Rust owns task truth, status
derivation, action eligibility, idempotency, and prompt safety. Nothing in this
checklist authorizes the browser to derive lifecycle, action validity, or status.

## API endpoints consumed (same-origin, relative URLs, `no-store`)

| Endpoint | Method | Used by (legacy) | Notes |
| --- | --- | --- | --- |
| `/api/health` | GET | `checkBackendHealth`, `waitForServerOnline`, diagnostics | Connection probe; text body |
| `/api/version` | GET | `checkForUpdate`, diagnostics | `{ version }`; drives stale-shell reload banner |
| `/api/cockpit` | GET | `loadCockpit`, diagnostics | `BrowserCockpitView` projection |
| `/api/tasks/{handle}` | GET | `loadDetail`, diagnostics | `BrowserTaskDetail`; 404 ⇒ task gone |
| `/api/tasks/{handle}/pane?since=N` | GET | `loadPane` | Pane delta; 404 degrade, 409 stale |
| `/api/tasks/{handle}/answer` | POST | `sendAnswer` | `{answer, fingerprint, request_id}`; 409/422/429 typed |
| `/api/tasks` | POST | `submitNewTask` | `{repo, title, agent, request_id}`; returns refreshed cockpit |
| `/api/operations` | POST | `runAction` | `{task_handle, action, request_id}`; returns refreshed cockpit |
| `/api/server/restart` | POST | `restartServer` | Triggers process restart |

## Dashboard / list view

- [ ] Cockpit polls every 1s (`REFRESH_INTERVAL_MS`); paused while `document.hidden`.
- [ ] Inbox ("Needs you") section sorted by ascending severity, filtered to selected project.
- [ ] Inbox card shows status dot, handle, status badge, explanation, and inline action row.
- [ ] Inbox card body (not buttons) is tap-to-open detail.
- [ ] Calm task list grouped by repo, repo title shown only when >1 repo and no project filter.
- [ ] Tasks sorted by status rank (running, waiting, error, idle) then handle.
- [ ] Tasks already in the inbox are not duplicated in the calm list.
- [ ] Status line summarizes total/attention counts, project-aware copy.
- [ ] Empty state shows "All quiet" or "No tasks in {project}".
- [ ] Structural fingerprint avoids full DOM rebuild when only live summaries change.

## Project filtering

- [ ] Project nav lists "All" plus every repo from cards and configured repos, sorted.
- [ ] Selecting a pill routes to `#/p/{repo}`; "All" routes to `#/`.
- [ ] Active pill reflects `selectedProject`.
- [ ] Filter applies to inbox, calm list, summary, and empty state.

## Task detail

- [ ] Detail polls every 1s; 404 ⇒ "Task no longer exists" + route to `#/`.
- [ ] Header shows back button (returns to project or dashboard) and title.
- [ ] Interact panel: status hero pill + explanation.
- [ ] "Task details" disclosure (branch, base, worktree, unpushed, agent, runtime, tmux) preserves open state across rerenders.
- [ ] Copyable branch and worktree rows.
- [ ] Structural fingerprint avoids rebuild; live summaries update in place.

## Pane / terminal interaction

- [ ] Pane polls on a state-aware cadence (1s active, 2.5s unchanged, 4s idle/hidden).
- [ ] Pane buffer bounded to `MAX_LOG_ENTRIES` (24) lines.
- [ ] Unchanged delta preserves existing lines; new lines appended.
- [ ] Task change clears the pane buffer and resets sequence.
- [ ] Missing tmux (`tmux_exists === false`) shows explicit "session unavailable" copy.
- [ ] Terminal output disclosure preserves open state across rerenders.
- [ ] Escape hatch shows `tmux attach -t {session}` with copy.
- [ ] "Copy visible output" and "Copy last error" shortcuts.

## Approvals / structured prompts

- [ ] Approve/Deny buttons render only when `pane.state.answerable` AND `pane.state.fingerprint` present.
- [ ] Otherwise: "Open the terminal below for this approval" hint.
- [ ] `WaitingForInput` shows the prompt text and a terminal hint (no buttons).
- [ ] `sendAnswer` includes `request_id` and `fingerprint`.
- [ ] 409 ⇒ "agent moved on" + immediate pane re-tick.
- [ ] 422 ⇒ "needs the terminal instead".
- [ ] 429 ⇒ "slow down" rate-limit message.

## Actions

- [ ] Only server-returned `actions` render; first action is visually primary.
- [ ] Destructive / confirmation-required actions need two taps; pending state survives rerender.
- [ ] Confirmation expires after `CONFIRM_TIMEOUT_MS` (8s).
- [ ] Running an action disables peer actions on the same card/detail only.
- [ ] Operation response with `cockpit` replaces projection; else `loadCockpit()`.
- [ ] Success ⇒ "{action} completed" + detail refresh; error ⇒ error result panel.

## New task

- [ ] Sheet repo select populated from `cockpit.repos.repos`.
- [ ] Selected project preselects the matching repo.
- [ ] Empty repo or title rejected locally.
- [ ] Request includes `request_id`.
- [ ] Server error renders inline + result banner, and applies returned `cockpit`.
- [ ] Success closes the sheet and applies returned `cockpit`.

## Settings & diagnostics

- [ ] Settings route `#/settings`.
- [ ] Restart requires confirmation (destructive two-tap), polls health up to `RESTART_TIMEOUT_MS` (30s) at `RESTART_POLL_MS` (500ms).
- [ ] Restart success routes to `#/` and reloads cockpit; timeout shows error.
- [ ] Diagnostics report includes browser mode, backend URL, versions, SW controller, cached results, and live checks.
- [ ] Copy uses clipboard with text-fallback when unavailable.

## Connection / recovery

- [ ] Connection states: connected, checking, reconnecting, disconnected, backend unreachable, stale session.
- [ ] `is-offline` body class toggles when not connected.
- [ ] Retry, Reload, Copy Diagnostics, Open Health URL controls.
- [ ] Health re-check on `online`, `visibilitychange`, `pageshow`, `focus`.
- [ ] Successful health re-check refreshes the current route.

## Shell version / update

- [ ] App version read from `<meta name="ajax-app-version">`.
- [ ] `/api/version` polled every 30s and on resume/focus/pageshow.
- [ ] Version mismatch reveals tap-to-reload update banner.

## Service worker / PWA

- [ ] `sw.js` is a self-unregistering cleanup worker (no fetch/cache/push/sync).
- [ ] Existing registrations are unregistered on load; no new registration.
- [ ] `/api/*` never intercepted.

## Mobile layout (Safari-first)

- [ ] Safe-area top chrome and bottom navigation.
- [ ] 320–390px width, portrait & landscape.
- [ ] Sheet keyboard behavior and focus return.
- [ ] Background ≥1 min then foreground refresh.
- [ ] Reduced-motion support.
</content>
</invoke>
