# Ajax Web Exploratory Testing (CI-only)

Cloud-only exploratory testing for Ajax Web. Everything runs on a
GitHub-hosted `ubuntu-latest` runner. It is intentionally **not** part of local
development, `npm test`, husky, or `npm run verify`.

## Secrets

| Secret | Required | Purpose |
| --- | --- | --- |
| `CURSOR_API_KEY` | yes | Authenticate the Cursor Agent CLI |

Create the secret in the repository settings (or via `gh secret set CURSOR_API_KEY`).
The workflow fails clearly when the secret is missing. Do not commit API keys.

## Workflow

`.github/workflows/exploratory-testing.yml`

- `schedule`: daily `17 6 * * *` UTC only (no manual dispatch)
- One Cursor explorer process per run (no relaunch / continuation)
- Job timeout: 45 minutes
- Artifacts uploaded with `if: always()`

## Architecture

```text
GitHub-hosted Actions VM
  ├── checkout Ajax
  ├── restore exploration memory (cache; optional)
  ├── install Node / Rust / Playwright WebKit / tmux / Cursor CLI
  ├── build Ajax Web + ajax-cli
  ├── prepare isolated ajax-cli web (target/exploratory-instance; agent stubs on PATH)
  ├── plan deterministic mission (primary + fallback)
  ├── start server + wait for readiness
  ├── seed mission product state via authenticated /api/* (POST /api/session first)
  ├── preflight fake ACP wrapper + seeded task verification when required
  ├── prepare oracles + mission prompt
  ├── Cursor Agent + Playwright MCP (WebKit) explores under charter
  ├── validate + classify findings (novel / known / regression)
  ├── update bounded campaign memory
  ├── file novel/regression confirmed findings
  └── fail on infrastructure, blocked preflight, or empty skeleton runs
```

The exploration budget is a **maximum**, not a target. WebKit only — no Chromium/Firefox fallback.

## Mission selection

`scripts/exploratory/plan-mission.mjs` writes `exploratory-results/mission.json`:

- Compares web-related changes since validated `memory.lastRunSha`
- Rotates least-recently tested missions
- Avoids missions tied to known confirmed fingerprints
- Emits one **primary** mission and one **fallback**

The prompt assigns exactly one mission per nightly run.

## Fake ACP preflight

Session missions seed tasks with agent **`cursor`**. Ajax launches Cursor via
`agent [--model ID] acp`; `scripts/exploratory/agent-stubs/agent` on the server
PATH delegates to `fake-acp`, which wraps `crates/ajax-web/tests/fixtures/fake_acp.js`.
`preflight-fake-acp.mjs` proves initialize + session/new + session/prompt through that
wrapper and, when the server is up, verifies the seeded task is readable.

## Intelligence model

Exploration is driven by **assigned mission + oracles**, not a codebase index.

Before each agent run, `scripts/exploratory/prepare-oracles.mjs` writes
`exploratory-results/oracles.json`:

| Oracle | Source |
| --- | --- |
| `openBugs` | Open GitHub `bug` issues (prefers Web Cockpit / `[defect]`) |
| `closedBugs` | Closed GitHub `bug` issues (regression fingerprints) |
| `recentWebCommits` | Last 20 web-related `git log` lines |
| `routes` | Static list matching `routes.ts` |
| `boundaryHashes` | Known routing defect neighborhood |
| `memory` | Durable corpus hints (`dullActions`, focus, fingerprints) |

## Agent constraints

- Model: **Composer 2.5** (`composer-2.5`) — fixed in the workflow and runner
- Charter: `.github/exploratory/charter.md`
- MCP: `.github/exploratory/mcp.json` (Playwright MCP, WebKit headless, iOS-ish viewport 390×844)
- CLI permissions: `.github/exploratory/cli.json` (WebKit MCP + results writes only; sandbox enabled, shell/network denied)
- Post-run `git` dirty check fails the job if product source changed

## Outputs

`exploratory-results/`

- `mission.json` — primary/fallback mission for this run
- `oracles.json` — oracle pack for this run
- `run.json` — run metadata, preflight, usefulness, infrastructure, classification, issue summary
- `findings.json` — confirmed / observation / rejected items (+ `classification` when confirmed)
- `issues.json` — GitHub issue filing results for eligible findings
- `observations.json` — lower-confidence notes
- `memory-delta.json` — adaptive hints for the next run
- `seed.json` — optional mission seeding via public API
- `traces/`, `screenshots/`, `logs/`

## Issue automation

After validate + classify succeed, `scripts/exploratory/file-issues.mjs` processes
**novel** and **regression** confirmed findings for creation, and records **known**
findings as duplicates (commenting only when materially new evidence signatures appear).
Agent `relatedIssues` are hints only and do not suppress filing.

Confirmed findings require:

- `reproductionSuccesses >= 2`
- a fingerprint
- evidence paths under `exploratory-results/`

See `issue-reporting.md`.

## Local note

Developers cloning Ajax should never auto-run this suite. Optional tooling
checks live under `scripts/exploratory-*.test.mjs` and only validate schemas /
mission helpers — they do not launch the agent or a browser.

Fixture-backed local checks:

```bash
node scripts/exploratory/prepare-instance.mjs
node scripts/exploratory/plan-mission.mjs
node scripts/exploratory/preflight-fake-acp.mjs
node scripts/exploratory/validate-run.mjs --fixture --skip-readonly
```
