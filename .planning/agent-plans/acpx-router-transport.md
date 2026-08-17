# Use acpx ACP as Ajax Model Router transport

## Approval

- Status: **approved** (user: “do it and make sure it keeps the model ajax
  model router wants to delegate to”).
- Request: use [openclaw/acpx](https://github.com/openclaw/acpx) ACP as the
  transport layer for Ajax Model Router.

## Problem

Ajax Model Router already decides *who* runs (`cursor` / `codex` / `pi`) but
talks to each harness through a custom subprocess:

- Cursor: `cursor-agent` stream-json
- Codex: Codex app-server
- Pi: `pi --mode rpc`

That is three parsers, three session/cancel stories, and PTY-shaped output.
The parent still has to know each tool’s flags. ACP is the structured
session protocol; `acpx` is the headless client for it.

## Target

One transport: **`acpx` speaking ACP**. Router `EXECUTION.AGENT` maps to an
acpx built-in profile, then one command shape:

```text
acpx <cursor|codex|pi> exec|prompt …  --cwd <worktree> --format json
```

| Router agent | acpx profile | ACP adapter acpx already ships |
| --- | --- | --- |
| `cursor` | `cursor` | `cursor-agent acp` |
| `codex` | `codex` | `@agentclientprotocol/codex-acp` |
| `pi` | `pi` | `npx pi-acp` |

Keep the router control plane (`EXECUTION`, SCOPE, snapshots, `DELEGATE_REPORT`,
parent review). Replace only `scripts/run-delegate` / `libexec/run_delegate.py`
and the three adapter launch paths.

Do **not** route Ajax Model Router through OpenClaw Gateway (`openclaw acp`).
That is the opposite direction (IDE → OpenClaw). We want Ajax as the
**ACP client** via `acpx`, driving Cursor/Codex/Pi as ACP servers.

## Why acpx (not raw ACP SDK)

- Same CLI for all three current targets, plus `--format json` NDJSON events.
- Sessions, `session/cancel`, cwd/fs boundary, permission modes for
  non-interactive runs.
- MIT, npm `acpx`, Node 22.13+ (Ajax already pins Node 22).
- Pre-1.0: pin a version; treat CLI as evolving.

## Non-goals

- No OpenClaw Gateway / Discord / `sessions_spawn` dependency.
- No pstack.
- No change to Ajax task lifecycle, registry, or Web Cockpit ACP chat
  (`crates/ajax-web` session ACP stays a separate product surface).
- Do not expand the model registry to every acpx built-in (Claude, Gemini,
  Copilot, …) in this change. Mapping is only the three existing router
  agents unless a follow-up is approved.
- Do not keep dual transports “just in case” after cutover.

## Design constraints

- Non-interactive: pick an acpx permission mode that cannot hang on prompts
  (approve-all or equivalent from acpx permissions docs). Fail closed if the
  agent asks for interactive auth.
- `--format json` (or quiet + report file) must still yield a parseable
  `DELEGATE_REPORT`. Prefer extracting the existing YAML markers from the
  agent’s final text; do not invent a second report schema.
- `exec` for one-shot bounded tasks (matches today’s no-saved-session Pi
  path). Named `acpx` sessions only if resume/`CHAT_ID` is already in the
  router transaction.
- `--cwd` = task worktree. That is the filesystem boundary.
- Pin `acpx` version in router docs/install; `command -v acpx` missing →
  `STOP`, no silent fallback to `cursor-agent` / app-server / `pi --mode rpc`.
- Codex today uses app-server + `--reasoning-effort xhigh`. ACP cutover must
  preserve model id from the registry; if ACP cannot set reasoning effort,
  document the gap in the PR — do not silently drop it without saying so.
- Contract tests today pin exact Pi/Cursor/Codex invocation strings. Those
  pins move to `acpx …` in the same change.

# acpx-router-transport.md tasks (implementation)
- [x] Replace run-delegate tool backends with a single acpx launcher
- [x] Stop calling Codex app-server and Pi RPC launchers from run-delegate
- [x] Update cursor/pi/codex delegate skills to name acpx
- [x] Update scripts/check-contracts and Python tests for acpx invocations
- [x] README + ajax-cli routing.md one-liner: dispatch transport is acpx ACP
- [ ] Spike: live acpx cursor/pi exec from fixture worktree (deferred; fake stub in CI)

## Verification

```bash
cd /Users/matt/Desktop/Projects/ajax-model-router && bash scripts/check-contracts
```

Plus a focused launcher test with a fake `acpx` stub (no live network agents
in CI).

## Remaining risk

- acpx pre-1.0 CLI drift.
- Codex ACP vs current app-server (reasoning effort, sandbox).
- Cursor `cursor-agent acp` vs today’s `cursor-agent -p` stream-json; prompt
  and resume (`CHAT_ID`) may not map 1:1.
- Permission popups in headless CI if the mode is wrong.

## Validation results

- None yet (planning only).
