# Plan: Cloud-only Ajax Web exploratory testing

> **Superseded** by `nightly-exploratory-defect-discovery.md` on 2026-08-26.
> Retained for historical reference only.

## Scope

- Scheduled / manually dispatched GitHub Actions exploratory testing for Ajax Web.
- Cursor Agent CLI + Playwright MCP against an isolated `ajax-cli web` instance.
- Structured findings artifacts + lightweight exploration memory via Actions cache.
- Duplicate-aware GitHub Defect issue filing for confirmed findings (post-validate).
- Oracle pack + charter-driven sessions (not coverage tours).
- No local hooks, no husky/verify integration.

## Non-goals

- Hard-coded Playwright regression suites.
- Codebase index / Graphify / codebase-intel MCP for exploration intelligence.
- Local/self-hosted runners or persistent browser hosts.
- Fixing defects or mutating product source during exploration.

## Layout

| Concern | Location |
| --- | --- |
| Workflow orchestration | `.github/workflows/exploratory-testing.yml` |
| Agent charter | `.github/exploratory/charter.md` |
| Browser MCP config | `.github/exploratory/mcp.json` |
| Agent CLI permissions | `.github/exploratory/cli.json` |
| Finding schema | `.github/exploratory/findings.schema.json` |
| Docs / secrets | `.github/exploratory/README.md` |
| Oracle pack | `scripts/exploratory/prepare-oracles.mjs` → `exploratory-results/oracles.json` |
| Instance + agent scripts | `scripts/exploratory/*` |
| Agent stubs (task tmux) | `scripts/exploratory/agent-stubs/*` |
| Output (artifact only) | `exploratory-results/` |

## Intelligence slice (oracles + charters)

- [x] `prepare-oracles.mjs` — open bugs, recent web commits, static routes/boundary hashes, memory hints
- [x] Charter rewrite — session charters, forbid nav smoke tour as whole session
- [x] `prepare-prompt.mjs` embeds oracles; charter start from bug neighborhood
- [x] Workflow step after wait-ready, before prepare-prompt
- [x] `run-agent.sh` relaunch continues charter / reads oracles; max 2 attempts; honors stop-reason; budget is a maximum
- [x] Playwright MCP viewport `390x844` (iOS Safari-ish WebKit)
- [x] `scripts/exploratory-oracles.test.mjs`

## Verification

- [x] `node scripts/verify-ci-workflows.mjs`
- [x] `node --test scripts/exploratory-helpers.test.mjs`
- [x] `node --test scripts/exploratory-file-issues.test.mjs`
- [x] `node --test scripts/exploratory-memory.test.mjs`
- [x] `node --test scripts/exploratory-oracles.test.mjs`
- [x] fixture `validate-run.mjs --fixture` produces structured findings
- [x] missing `CURSOR_API_KEY` fails cleanly (exit 2)
- [x] workflow YAML parses; `runs-on: ubuntu-latest`; timeout 45m
- [x] no husky / package.json hooks invoke the explorer
- [x] prepare-instance seeds `origin/main` on demo repo (bare sibling remote)
- [x] workflow installs tmux; agent stubs on ajax-cli web PATH only
- [x] update-memory merges object `areasVisited`, `repoSha`, `observations.json`
- [x] run-agent relaunches at most once on early clean exit (≥60s attempt, ≥120s remaining, no stop-reason)
- [x] default exploration budget 12 minutes (maximum); WebKit-only MCP; assert-webkit preflight
- [ ] Full GHA run with real `CURSOR_API_KEY` (requires secret in repo)

## Remaining

- First manual `workflow_dispatch` after `CURSOR_API_KEY` is set
- Confirm charter-driven exploration on a full GHA run (blocked on secret)
