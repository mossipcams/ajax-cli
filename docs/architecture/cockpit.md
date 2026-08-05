# Native Cockpit Architecture

Native TUI Cockpit views and composition over core projections.


Cockpit is the operator surface over the JSON-backed command boundary.

`ajax-tui` owns native terminal interaction and rendering.

`ajax-web` owns browser interaction and rendering. Native Cockpit and Web
Cockpit are sibling presentation adapters over shared core projections and
actions; neither surface owns task truth. `ajax-tui` must not know about HTTP,
TLS, browser shell assets, or static web assets.

Web Cockpit serves HTTPS so browsers treat it as a secure context. On first run
it generates a self-signed certificate and persists it beside the state
database; the operator trusts it once on the browser device. HTTPS does not
require Home Screen installation. Optional Declarative Web Push (Web Cockpit
only) needs Add to Home Screen on a capable browser; see
[`web-cockpit.md`](web-cockpit.md).

Native Cockpit starts `ajax-cli web` by default and keeps it alive for the
Cockpit session. `ajax-cli` starts Web Cockpit on port `8787` with the stable
state database, while `ajax-cli dev` starts it on port `8788` with the
development state database. `--no-web` disables Web Cockpit startup. The web
process is started with explicit `AJAX_PROFILE`, `AJAX_CONFIG`, `AJAX_STATE`,
and rooted worktree values from the selected Ajax context so stable and dev
browser sessions stay on their own runtime profile.

- `actions` owns action and annotation chrome metadata.
- `cockpit_state` owns view state, selectable construction, transitions,
  refresh application, short-lived cockpit response caching, flash state, and
  confirmations.
- `input` owns terminal-event classification.
- `layout` owns pure layout calculations.
- `navigation` owns key classification helpers.
- `rendering` owns status palette, glyph mapping, and screen rendering.
- `runtime` owns terminal mode, polling, refresh timing, the cockpit refresh
  cache window, and the event loop.

### Cockpit Views

Cockpit has three navigational views:

- `Projects` — top level. Shows the cross-repo annotation inbox followed by
  the repo list and any unannotated tasks. Inbox rows surface tasks needing
  operator attention regardless of repo.
- `Project` — a single repo's task list. Each task row carries its handle,
  annotation label (or live summary), and primary-action chrome.
- `NewTaskInput` / `Help` — modal text input and reference screen.

There is no separate per-task action menu view. Enter on a task or inbox row
expands an inline drawer that lists the task's available operator actions
underneath the row; Enter on a drawer row dispatches that action. Esc or
selecting a different task collapses the drawer.
