# F — Feature cost inventory

Slice: measure total production + test lines per product feature (Rust, TS, CSS,
scripts, docs), end to end. Input to a product decision; nothing here is
authorized for removal. Every removal below is **PUBLIC**.

Method: `git ls-files` to enumerate tracked files per feature, `wc -l` on each
named file; caller searches with `rg`. Where a feature's wiring into shared
files (route tables, settings views, API clients) could not be counted line by
line it is marked *estimated* and the candidate is labeled "likely" instead of
"proven". Files named by earlier slices A/B2/B3/C/D/E are not re-credited here.

Baseline: 257,659 tracked lines (104k prod / 152k test). Target 38,650.

---

### F-01 Web terminal tab + mobile helpers
- Paths: `crates/ajax-web/web/src/features/terminal/*`, `crates/ajax-web/web/src/shared/lib/terminal*.{ts,tsx}`, `crates/ajax-web/src/adapters/terminal_pty/*`, `crates/ajax-web/src/slices/terminal.rs`
- Prod lines removed: ~4,100 (features/terminal prod 2,160 + shared/lib terminal* prod 911 + terminal_pty adapter+slice 1,876 raw minus 846 test file = 1,030)
- Test lines removed: ~3,956 (features/terminal tests 1,128 + shared/lib terminal* tests 1,982 + terminal_pty/tests.rs 846)
- Evidence: `git ls-files crates/ajax-web/web/src/features/terminal | grep -v test | xargs wc -l` → 2160; same with `grep test` → 1128. `git ls-files .../shared/lib | grep terminal*` prod 911 / tests 1982. `git ls-files crates/ajax-web/src/adapters/terminal_pty crates/ajax-web/src/slices/terminal.rs | xargs wc -l` → 1876 (tests.rs is 846). Reachability: tab is wired in `App.tsx`/Cockpit and uses the `/api/...` pty routes served by the adapter; this slice did not re-derive every route.
- What replaces it, or why nothing needs to: nothing in-repo. The native TUI (`ajax cockpit`) and `tmux attach` still exist (both frozen surfaces), but an in-browser terminal — especially on mobile — has no equivalent. Removing it loses the Cockpit terminal tab entirely.
- Behavior or public contract change: **PUBLIC** — terminal tab disappears from web Cockpit desktop and mobile; mobile helpers (touch selection, keyboard handling) go with it.
- Risk: low mechanically (self-contained feature dir + adapter), high product-wise: Chat-first doc says "Terminal is a tab (lazy tmux)" — an explicit current product element, so this is **supported-but-separable**, not leftover.
- Depends on: none
- Confidence: proven (all named files counted; shared-file wiring uncounted and small)

### F-02 Training page + training slice
- Paths: `crates/ajax-web/src/slices/training/*`, `crates/ajax-web/web/src/features/training/*`, wiring in `web/src/shared/lib/routes.ts`, `polling.ts`, `types.ts`, `app/App.tsx`, `styles/settings.css`
- Prod lines removed: ~2,880 (Rust training dir 1,207 raw − 676 test file = 531; web feature prod 2,201 from per-file wc; wiring *estimated* ~150)
- Test lines removed: ~3,380 (Rust tests.rs 676 + web feature tests 2,522 + App.training.test.tsx 85 + wiring tests *estimated* ~100)
- Evidence: `git ls-files crates/ajax-web/src/slices/training | xargs wc -l` → 1207. Per-file `wc -l` of the 12 non-test web files sums to 2,201; test files sum to 2,522. Reachability: `rg -li training crates/ajax-web/web/src -g '!*features/training*'` → routes.ts, App.tsx, polling.ts, types.ts (route + page registration); Rust side reached via `slices/mod.rs`, `runtime/mod.rs`, `runtime/state.rs`.
- What replaces it: nothing. It is the "training command center" (planning docs exist under `.planning/agent-plans/training-*`), i.e. a product surface, not scaffolding. Removing it loses dataset/eval/pipeline training UI and its API.
- Behavior or public contract change: **PUBLIC** — `/training` route and `/api/training*` endpoints removed; `docs/architecture/web-cockpit.md` references it.
- Risk: medium — self-contained dirs, but referenced from shared app files and an architecture doc that must be updated in the same change.
- Label: **supported-but-separable** (distinct surface from chat; not described as a current direction in `architecture.md`)
- Depends on: none
- Confidence: likely (wiring lines estimated, not counted)

### F-03 Speech input (STT + moonshine sidecar)
- Paths: `crates/ajax-web/src/adapters/stt_provider/*`, `crates/ajax-web/src/slices/stt.rs`, `web/src/shared/lib/speech*.ts`, `web/src/shared/hooks/useSpeechInput.ts`, `web/src/features/chat/composer/speech/*`, `web/src/features/terminal/useTaskTerminalSpeech.{ts,tsx}`, `scripts/ajax-moonshine-sidecar`, `scripts/setup-stt.sh`, `docs/speech-input.md`
- Prod lines removed: ~3,320 (Rust 1,487 − 298 test file = 1,189; web prod 1,383 from per-file wc; scripts 518; docs 235)
- Test lines removed: ~1,680 (stt_provider/tests.rs 298 + web speech tests 1,379 + inline `#[cfg(test)]` in stt.rs uncounted)
- Evidence: `git ls-files crates/ajax-web/src/adapters/stt_provider crates/ajax-web/src/slices/stt.rs | xargs wc -l` → 1487. Web speech files per-file wc = 25+36+45+302+35+287+653 = 1,383; tests 1,379. `wc -l scripts/ajax-moonshine-sidecar scripts/setup-stt.sh docs/speech-input.md` → 401/117/235. Reachability: mic button in chat composer and terminal tab calls `useSpeechInput` → `/api/stt*` routes → sidecar process; doc `docs/speech-input.md` describes it as a shipped feature.
- What replaces it, or why nothing needs to: a cheaper native path exists — the browser's Web Speech API / on-device dictation removes the 401-line moonshine sidecar and most of the transport code, but latency/privacy characteristics change and mobile coverage is uneven. Honest answer for full removal: nothing replaces voice input.
- Behavior or public contract change: **PUBLIC** — mic button gone from chat composer and terminal; `docs/speech-input.md` deleted; `setup-stt.sh` goes away.
- Risk: medium — feature is isolated (own adapter dir, own scripts) but touches the shared chat composer.
- Label: **supported-but-separable**
- Depends on: partially entangled with F-01 only via `useTaskTerminalSpeech.tsx` (45+ lines, goes either way)
- Confidence: likely (inline Rust tests and composer wiring estimated)

### F-04 Claude SDK chat harness
- Paths: `crates/ajax-web/sidecar/claude_sdk_sidecar.mjs`, `crates/ajax-web/src/adapters/web_session_acp/claude_*.{rs}` (client, map, spawn, sidecar tests), `session_client_claude_tests.rs`, `catalog*.rs`
- Prod lines removed: ~1,390 (4,987 raw across named files minus 3,594 in explicitly-named test files)
- Test lines removed: ~3,600 (claude_sdk_client_tests 492 + hardening 117 + map tests 830+161 + spawn_tests 209 + sidecar override/resolution/resume/main tests 151+140+210+893 + session_client_claude_tests 326 + catalog_claude_sdk_tests 65 = 3,594)
- Evidence: per-file `wc -l` of the named files sums to 4,987 including the 598-line `.mjs` sidecar. Reachability: harness is selected in the web_session harness dispatch (`catalog.rs`, `session_client.rs`) and exercised live; it is one of several selectable chat harnesses, not dead code.
- What replaces it, or why nothing needs to: a cheaper path exists — drive Claude Code through the *generic* ACP/stdio plumbing (shared `client.rs`/`rpc_*` family) instead of the dedicated SDK client + `.mjs` sidecar; that collapses ~600 lines of sidecar JS and the map layer if protocols align. Without that convergence, removal loses Claude as a selectable chat harness.
- Behavior or public contract change: **PUBLIC** — "Claude" option disappears from the harness/model picker unless replaced by the generic path.
- Risk: high — this is one of the primary chat paths; protocol-parity with the generic ACP client must be proven before removal.
- Label: **core** (primary harness) if it is your main agent; **supported-but-separable** otherwise
- Depends on: F-07 (generic ACP core stays)
- Confidence: likely (shared-file boundaries between per-harness and shared code estimated)

### F-05 Cursor SDK chat harness
- Paths: `crates/ajax-web/sidecar/cursor_sdk_sidecar.mjs`, `sidecar/cursor_sdk_login*.mjs`, `adapters/web_session_acp/cursor_*.{rs}`, `session_client_cursor_tests.rs`
- Prod lines removed: ~1,710 (4,263 raw named-file lines − 2,553 in named test files)
- Test lines removed: ~2,550 (cursor_login_tests 473 + client_tests 317 + map_tests 446 + spawn_tests 137 + session_client_cursor_tests 237 + mjs tests 428+515 = 2,553)
- Evidence: per-file `wc -l` sums to 4,263 including sidecars (377+162+428+515 = 1,482). Reachability: same harness dispatch as F-04; login flow reachable from web auth UI (`cursor_login.rs`, 298 lines).
- What replaces it, or why nothing needs to: same story as F-04 — generic ACP/stdio drive of `cursor-agent` is the cheaper native alternative and would delete the two `.mjs` sidecars (~967 raw lines incl. tests) outright. Without it, removal loses Cursor as a chat harness plus its login flow.
- Behavior or public contract change: **PUBLIC** — "Cursor" harness + its login UI gone.
- Risk: high — primary harness for much of this repo's own workflow (model-router skills dispatch to cursor-delegate); do not remove before the generic path is demonstrated.
- Label: **core** in current practice; separable only after generic ACP covers it
- Depends on: F-07
- Confidence: likely

### F-06 Push notifications (VAPID webpush)
- Paths: `crates/ajax-web/src/slices/push.rs`, `push/delivery.rs`, `web/src/features/settings/pushTest.{ts,tsx}`, push sections of `web/src/shared/lib/api.ts` and `features/settings/SettingsView.tsx`
- Prod lines removed: ~1,380 (Rust 1,051 raw; web pushTest 201 + api.ts/SettingsView push wiring *estimated* ~130)
- Test lines removed: ~260+ (pushTest.test.ts 209; Rust inline `#[cfg(test)]` in push.rs/delivery.rs uncounted — the 797-line push.rs contains more than pure prod code, so real prod is likely under 1,051)
- Evidence: `git ls-files crates/ajax-web/src/slices/push* | xargs wc -l` → 1051. `rg -n "push" web/src/shared/lib/api.ts` shows `/api/push/vapid|subscribe|test` client calls; `SettingsView.tsx` has 11 push references (subscription UI). No service worker is tracked (`git ls-files | grep -iE "sw\.js|service.?worker"` → empty), so the server-side VAPID hub is the whole push story.
- What replaces it, or why nothing needs to: a cheaper native alternative exists — browser `Notification` API with poll-based wake while the page/PWA is open; what is genuinely lost is *remote* notification when no tab is open.
- Behavior or public contract change: **PUBLIC** — push subscription settings, "test push" button, `/api/push/*` routes, VAPID key files all removed.
- Risk: medium — isolated slice; mobile users lose remote chat-event alerts.
- Label: **supported-but-separable** (architecture.md says "notifications are chat events"; delivery channel is separable from the event itself)
- Depends on: none
- Confidence: likely

### F-07 Generic ACP / session-shim core (reference — stays)
- Paths: `crates/ajax-web/src/adapters/web_session_acp/{client,rpc_*,jsonl_process,session_client,sdk_connection,sdk_elicitation,config_*,catalog}.rs`, `adapters/session_shim/*`
- Lines measured: ~24,000 across the two dirs (total 24,035 from `git ls-files ... | xargs wc -l`)
- Evidence: counted above; this is the load-bearing path every harness rides on and the session-shim contract from architecture.md Chat-first ("per-session shim… one Unix socket that replays from an offset").
- Not a removal candidate. It sets the floor: any per-harness removal (F-04/F-05/F-08) is only worth doing if it converges *onto* this, not around it.
- Label: **core**
- Confidence: proven

### F-08 Pi RPC chat harness
- Paths: `crates/ajax-web/src/adapters/web_session_acp/pi_rpc_{client,handshake,map}*.rs`, `catalog_pi_rpc_tests.rs`
- Prod lines removed: ~950 (2,775 raw named-file lines − 1,823 in named test files)
- Test lines removed: ~1,820 (options 162 + client tests 381 + usage 222 + handshake tests 199 + map tests 727 + catalog pi rpc tests 132 = 1,823)
- Evidence: per-file `wc -l` of the nine named files = 2,775. Reachability: harness dispatch like F-04/F-05; separate protocol family (pi RPC) not shared with the SDK clients.
- What replaces it, or why nothing needs to: drive Pi through generic ACP/stdio if supported; otherwise removal loses Pi as a chat harness. Nothing in-repo replaces it for Pi-model sessions.
- Behavior or public contract change: **PUBLIC** — "Pi" harness option removed.
- Risk: medium — self-contained protocol family, smaller than the SDK harnesses.
- Label: **supported-but-separable**
- Depends on: F-07
- Confidence: likely

### F-09 dev_deploy + test-in-stable (dev-only deploy workflow)
- Paths: `crates/ajax-web/src/slices/dev_deploy/*`, `scripts/dev-web-restart.{sh,test.mjs}`, `scripts/test-in-stable.sh`, `web/src/features/task/TestInDevPanel.{tsx,ts.test.tsx}`
- Prod lines removed: ~1,090 (Rust 485 + scripts 527 (487 sh + 40 test-in-stable) + TestInDevPanel.tsx 76)
- Test lines removed: ~445+ (dev_deploy/tests.rs 307 + dev-web-restart.test.mjs 138; TestInDevPanel.test.tsx exists but was not line-counted)
- Evidence: `git ls-files crates/ajax-web/src/slices/dev_deploy scripts/test-in-stable.sh | xargs wc -l` → 485/307/40. Reachability: web UI panel (`TestInDevPanel.tsx`, 76 lines) + `ajax service` interplay; dev-only surface for deploying the dev web server from the product UI and "test in stable" script.
- What replaces it, or why nothing needs to: plain shell commands / docs cover the same operations (restart dev web, run in stable checkout). Removing it loses one-click deploy-from-UI during development — a convenience, not product behavior.
- Behavior or public contract change: **PUBLIC** — dev-deploy panel and endpoints removed; `scripts/test-in-stable.sh` deleted (dev workflow only, no end-user contract).
- Risk: low — developer-only surface.
- Label: **leftover-ish / supported-but-separable** (a development affordance that outgrew its purpose; cheap to drop)
- Depends on: none
- Confidence: likely

### F-10 launchd web service (`ajax service *`)
- Paths: `crates/ajax-cli/src/web_service/{mod,tests}.rs`
- Prod lines removed: 463; Test lines removed: 533 (both files counted directly)
- Evidence: `git ls-files crates/ajax-cli/src/web_service* | xargs wc -l` → 996. Reachability: `ajax service install/uninstall/start/stop/status` subcommand table in `crates/ajax-cli/src/cli.rs:272-287`.
- What replaces it, or why nothing needs to: running the web server manually (or a documented launchd plist snippet in docs) covers it; removing loses zero runtime capability, only the install/uninstall verbs and their status integration.
- Behavior or public contract change: **PUBLIC** — five CLI verbs removed from `ajax service`.
- Risk: low — single module, self-contained; operator muscle memory is the main cost.
- Label: **supported-but-separable**
- Depends on: none
- Confidence: proven

### F-11 TLS + browser session + Cloudflare Access adapters
- Paths: `crates/ajax-web/src/adapters/tls.rs` (136), `browser_session.rs` (193), `cloudflare_access.rs` (240)
- Prod lines removed: 569 total; only the Cloudflare Access piece (~240 + its config wiring in `runtime/state.rs`) is separable — TLS and browser session are required to serve securely.
- Test lines removed: uncounted (covered inline by runtime suites, e.g. `runtime/tests/suite_1.rs:477 cloudflare_access_enabled_rejects_missing_jwt_on_protected_routes`)
- Evidence: per-file `wc -l` = 569. Reachability: wired in `runtime/mod.rs`, `runtime/state.rs`; CF Access is an opt-in deploy mode (JWT check on protected routes).
- What replaces it, or why nothing needs to: for Cloudflare Access specifically, fronting the server with a real reverse proxy / WAF is the cheaper native alternative. TLS and browser-session cookie handling cannot be replaced by anything in-repo; they are required infrastructure.
- Behavior or public contract change: **PUBLIC** (CF Access only) — `CF_ACCESS_*`-style config keys and JWT verification removed.
- Risk: medium for CF Access (deploy-mode option); TLS/browser session removal is out of scope entirely.
- Label: **supported-but-separable** (CF Access), **core** (TLS, browser session)
- Depends on: none
- Confidence: proven for line counts; separability is likely

### F-12 Cost / usage reporting
- Paths: `crates/ajax-web/src/slices/web_session/acp_usage{,_tests}.rs`, `web/src/features/chat/status/UsageIndicators.{tsx,ts.test.tsx}`, fragments of `styles/chat/{activity,status}.css`
- Prod lines removed: ~190 (acp_usage.rs 159 + UsageIndicators.tsx 13 + CSS fragments uncounted)
- Test lines removed: ~400 (acp_usage_tests.rs 378 + UsageIndicators.test.tsx 17)
- Evidence: `wc -l` → 537 total for the Rust pair; UsageIndicators is rendered from `LiveHead.tsx` / status projection (`rg -ln UsageIndicators web/src` → public.ts, LiveHead).
- What replaces it, or why nothing needs to: nothing — removal loses per-turn token/cost visibility in chat status. Cheaper path: harnesses already surface usage via ACP; this slice is the thin presentation of it, so shrinking further would mean dropping the feature.
- Behavior or public contract change: **PUBLIC** — usage/cost indicators disappear from chat status.
- Risk: low mechanically; small product visibility loss. Under the 150-line family floor on its own; included because the task named it and it is a complete, self-contained feature.
- Label: **supported-but-separable**
- Depends on: F-07 (usage flows through ACP events)
- Confidence: likely

---

## Absent / not candidates

- **ntfy notify**: `rg -c ntfy crates scripts docs` → no matches; `git ls-files | grep -i ntfy` → empty. This feature does not exist in the repo (or was already removed). 0 lines. Stated plainly: I did not find it anywhere tracked.
- **Skills**: all `skills/` entries under `.agents .claude .codex .cursor` are git symlinks (mode 120000, 0 lines each — `git ls-files -s | grep skills`), plus `adapters/skills.rs` at 45 lines. Nothing to remove.
- **Sandbox adapter** (`crates/ajax-core/src/adapters/sandbox.rs`, 832 lines): execution-safety boundary (trusted git, agent profile grants) referenced by `docs/architecture/durable-execution.md`. **core — keep.**
- **`ajax` CLI verbs the web now covers**: the ajax-cli crate's remaining surface (`agent_hooks.rs` 781, `cockpit_backend.rs`, `tmux_task_session/*`, `execution_dispatch.rs`) is either frozen Cockpit/TUI support code (slice A) or infrastructure the web does *not* replace. The one genuine overlap — `ajax service *` launchd verbs — is already counted as F-10. No separate candidate.

## Totals by confidence

| ID | Feature | Prod ~ | Tests ~ | Confidence |
|---|---|--:|--:|---|
| F-01 | Web terminal tab + mobile | 4,100 | 3,956 | proven (dirs) / wiring est. → **likely overall** |
| F-02 | Training page+slice | 2,880 | 3,380 | likely |
| F-03 | STT + moonshine sidecar | 3,320 | 1,680 | likely |
| F-04 | Claude SDK harness | 1,390 | 3,594 | likely |
| F-05 | Cursor SDK harness | 1,710 | 2,553 | likely |
| F-06 | Push notifications | 1,380 | 260+ | likely |
| F-08 | Pi RPC harness | 950 | 1,823 | likely |
| F-09 | dev_deploy + test-in-stable | 1,090 | 445+ | likely |
| F-10 | launchd web service | 463 | 533 | proven |
| F-11 | TLS/browser session/CF (CF part) | ~240+est. wiring | uncounted inline | proven counts / likely split |
| F-12 | Cost/usage reporting | 190 | 400 | likely |

Totals: **~17,700 prod / ~18,600 test ≈ 36,300 lines** if *every* separable feature above were approved — i.e. the separable surface is roughly the whole 15% budget. You will not take all of it; this is the menu with prices.

Proven-only subtotal: F-10 (996) + F-11 prod counts (569) ≈ 1,565 lines with no estimates involved.

## Not worth it (looked, should stay)

- **Generic ACP / session-shim core (~24k)** — the chat path itself; architecture.md Chat-first contracts live on it.
- **Sandbox adapter (832)** — execution safety, doc'd in durable-execution.md.
- **TLS + browser session (329 of the 569)** — required to serve at all.
- **ajax-core harness/agent abstraction (`adapters/agent.rs` etc.)** — core owns task truth incl. agent definitions; removing blurs ownership boundaries.
- **Skills symlinks** — zero tracked lines already.

## Not reached / budget note

- Inline `#[cfg(test)]` lines inside prod files (push.rs, stt.rs, all harness client files) were not subtracted per file; Rust "prod" numbers above are upper bounds by exactly that amount (repo-wide the inline-test share is ~20% of prod-file lines).
- Shared-file wiring (routes.ts, SettingsView push section, api.ts push functions, LiveHead usage fragments) estimated from `rg -c` hit counts, not line-extracted.
- CSS line attribution per feature (chat status.css, settings.css fragments) not extracted.
- Did not trace web routes for the terminal pty endpoint-by-endpoint (relied on adapter wiring).
- `TestInDevPanel.test.tsx` and runtime-suite CF Access test lines not counted individually.
