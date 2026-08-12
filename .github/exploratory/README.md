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

- `schedule`: daily UTC
- `workflow_dispatch`: manual runs from GitHub Actions UI (default budget 12 minutes; overridable)
- Job timeout: 45 minutes
- Artifacts uploaded with `if: always()`

## Architecture

```text
GitHub-hosted Actions VM
  ├── checkout Ajax
  ├── restore exploration memory (cache; optional)
  ├── install Node / Rust / Playwright WebKit / tmux / Cursor CLI
  ├── build Ajax Web + ajax-cli
  ├── start isolated ajax-cli web (target/exploratory-instance; agent stubs on PATH)
  ├── prepare oracles (open bugs, recent commits, routes, boundary hashes, memory)
  ├── Cursor Agent + Playwright MCP (WebKit) explores under charter (at most one continuation)
  ├── update memory + upload exploratory-results/
  └── fail only on infrastructure/explorer failure (not every product bug)
```

The exploration budget is a **maximum**, not a target. The agent may stop early when
stopping criteria apply (`stop-reason.json`). WebKit only — no Chromium/Firefox fallback.

## Intelligence model

Exploration is driven by **oracles + time-boxed charters**, not a codebase index.

Before each agent run, `scripts/exploratory/prepare-oracles.mjs` writes
`exploratory-results/oracles.json`:

| Oracle | Source |
| --- | --- |
| `openBugs` | Open GitHub `bug` issues (prefers Web Cockpit / `[defect]`) |
| `recentWebCommits` | Last 20 web-related `git log` lines (always, even first run) |
| `routes` | Static list matching `routes.ts` |
| `boundaryHashes` | Known routing defect neighborhood |
| `memory` | Durable corpus hints (`dullActions`, focus, fingerprints) |

The charter (`.github/exploratory/charter.md`) requires **session-based**
exploratory testing: pick one charter (**Happy path**, **Garbage hashes**,
**Interruption**, **Contradiction**, **Recovery**), probe until stopping criteria
apply, and only continue if high-value work remains. A one-click nav tour is
explicitly forbidden as the whole session. One run need not be exhaustive.

## Agent constraints

- Model: **Composer 2.5** (`composer-2.5`) — fixed in the workflow and runner
- Charter: `.github/exploratory/charter.md`
- MCP: `.github/exploratory/mcp.json` (Playwright MCP, WebKit headless, iOS-ish viewport 390×844)
- CLI permissions: `.github/exploratory/cli.json` (deny source edits / git mutation)
- Post-run `git` dirty check fails the job if product source changed

## Outputs

`exploratory-results/`

- `oracles.json` — oracle pack for this run
- `run.json` — run metadata, infrastructure status, and issue-filing summary
- `findings.json` — confirmed / observation / rejected items
- `issues.json` — GitHub issue filing results for confirmed findings
- `observations.json` — lower-confidence notes
- `memory-delta.json` — adaptive hints for the next run
- `stop-reason.json` — optional early-stop reason when marginal value is low
- `traces/`, `screenshots/`, `logs/`

## Issue automation

After validate succeeds, `scripts/exploratory/file-issues.mjs` files confirmed
findings as GitHub Defect issues (duplicate-aware). The workflow grants
`issues: write` (contents stays `read`). See `issue-reporting.md`.

Artifacts include `issues.json` alongside `findings.json`.

## Local note

Developers cloning Ajax should never auto-run this suite. Optional tooling
checks live under `scripts/exploratory-*.test.mjs` and only validate schemas /
memory / oracle helpers — they do not launch the agent or a browser.
