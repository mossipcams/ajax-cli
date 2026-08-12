#!/usr/bin/env bash
# Launch Cursor Agent for exploratory testing with a hard time budget.
# Uses exploratory MCP + CLI permission overlays under $HOME/.cursor.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPTS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS="$ROOT/exploratory-results"
PROMPT_FILE="$RESULTS/prompt.txt"
AGENT_LOG="$RESULTS/logs/agent.log"
BUDGET_MINUTES="${AJAX_EXPLORATORY_BUDGET_MINUTES:-25}"
MODEL="composer-2.5"

mkdir -p "$RESULTS/logs" "$HOME/.cursor"
cd "$SCRIPTS"

write_agent_status() {
  local exit_code="$1"
  local error_message="${2:-}"
  EXIT_CODE="$exit_code" ERROR_MESSAGE="$error_message" MODEL="$MODEL" BUDGET_MINUTES="$BUDGET_MINUTES" \
    node --input-type=module <<'EOF'
import { join } from "node:path";
import { readJson, resultsDir, writeJson } from "./lib.mjs";

const exitCode = Number(process.env.EXIT_CODE);
const errorMessage = process.env.ERROR_MESSAGE || null;
const timedOut = exitCode === 124;
const run = readJson(join(resultsDir, "run.json"), {});

run.agent = {
  status:
    errorMessage === "missing CURSOR_API_KEY"
      ? "failed"
      : timedOut
        ? "budget_exhausted"
        : exitCode === 0
          ? "completed"
          : "failed",
  exitCode,
  finishedAt: new Date().toISOString(),
  model: process.env.MODEL,
  budgetMinutes: Number(process.env.BUDGET_MINUTES),
  error: errorMessage,
};

if (errorMessage || (!timedOut && exitCode !== 0)) {
  run.infrastructure = {
    ...(run.infrastructure ?? {}),
    status: "failed",
    error: errorMessage ?? `agent exited with code ${exitCode}`,
  };
} else if (timedOut) {
  run.infrastructure = {
    ...(run.infrastructure ?? {}),
    status: run.infrastructure?.status === "failed" ? "failed" : "ok",
    error: run.infrastructure?.error ?? null,
  };
}

writeJson(join(resultsDir, "run.json"), run);
EOF
}

if [[ -z "${CURSOR_API_KEY:-}" ]]; then
  echo "::error title=missing secret::CURSOR_API_KEY is required for exploratory testing" >&2
  write_agent_status 2 "missing CURSOR_API_KEY"
  exit 2
fi

if [[ ! -f "$PROMPT_FILE" ]]; then
  echo "missing prompt file: $PROMPT_FILE" >&2
  write_agent_status 1 "missing prompt file"
  exit 1
fi

cp "$ROOT/.github/exploratory/mcp.json" "$HOME/.cursor/mcp.json"

node --input-type=module <<'EOF'
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { exploratoryDir } from "./lib.mjs";

const homeConfigPath = join(process.env.HOME, ".cursor", "cli-config.json");
const exploratory = JSON.parse(
  readFileSync(join(exploratoryDir, "cli.json"), "utf8"),
);
const base = existsSync(homeConfigPath)
  ? JSON.parse(readFileSync(homeConfigPath, "utf8"))
  : { version: 1 };
const next = {
  ...base,
  permissions: exploratory.permissions,
  approvalMode: exploratory.approvalMode ?? "allowlist",
  attribution: exploratory.attribution,
  sandbox: exploratory.sandbox,
};
writeFileSync(homeConfigPath, `${JSON.stringify(next, null, 2)}\n`);
EOF

AGENT_BIN="$(command -v agent || true)"
if [[ -z "$AGENT_BIN" ]]; then
  AGENT_BIN="$(command -v cursor-agent || true)"
fi
if [[ -z "$AGENT_BIN" ]]; then
  echo "Cursor Agent CLI not found on PATH (agent/cursor-agent)" >&2
  write_agent_status 1 "cursor agent cli missing"
  exit 1
fi

BUDGET_SECONDS=$((BUDGET_MINUTES * 60))
DEADLINE=$((SECONDS + BUDGET_SECONDS))
ATTEMPTS=0
MAX_ATTEMPTS=8
MIN_ATTEMPT_SECONDS=60
EXIT_CODE=0
CONTINUATION_SUFFIX=""

set +e
while true; do
  ATTEMPTS=$((ATTEMPTS + 1))
  REMAINING=$((DEADLINE - SECONDS))
  if (( REMAINING <= 0 )); then
    EXIT_CODE=124
    break
  fi

  if (( ATTEMPTS > 1 )); then
    echo "" >>"$AGENT_LOG"
    echo "=== exploratory relaunch attempt ${ATTEMPTS}/${MAX_ATTEMPTS} (~$((REMAINING / 60))m remaining) ===" >>"$AGENT_LOG"
  fi

  ATTEMPT_STARTED=$SECONDS
  # Process-group timeout so Playwright MCP children die with the agent.
  timeout --signal=TERM --kill-after=60s "${REMAINING}s" \
    "$AGENT_BIN" \
    --print \
    --trust \
    --approve-mcps \
    --force \
    --model "$MODEL" \
    --output-format text \
    --workspace "$ROOT" \
    "$(cat "$PROMPT_FILE")${CONTINUATION_SUFFIX}" \
    >>"$AGENT_LOG" 2>&1
  EXIT_CODE=$?
  ATTEMPT_DURATION=$((SECONDS - ATTEMPT_STARTED))

  if [[ "$EXIT_CODE" -eq 124 ]]; then
    break
  fi
  if [[ "$EXIT_CODE" -ne 0 ]]; then
    break
  fi

  REMAINING=$((DEADLINE - SECONDS))
  if (( REMAINING < 120 )); then
    break
  fi
  # Instant-exit agents must not spin the remaining budget.
  if (( ATTEMPT_DURATION < MIN_ATTEMPT_SECONDS )); then
    echo "agent returned in ${ATTEMPT_DURATION}s; not relaunching" >>"$AGENT_LOG"
    break
  fi
  if (( ATTEMPTS >= MAX_ATTEMPTS )); then
    break
  fi

  CONTINUATION_SUFFIX=$'\n\n---\n\nContinuation: ~'"$((REMAINING / 60))"$' minutes remain in the exploration budget. Read existing exploratory-results/ (findings, observations, memory-delta) and exploratory-results/oracles.json first. Continue the current charter or pick the next from oracles + suspicion — do not restart a first-pass area tour or dashboard click-tour. Skip dullActions from oracles/memory. The finish checklist is not permission to stop early.'
done
set -e

write_agent_status "$EXIT_CODE"

# Budget timeout is a successful exploration stop if the wrapper reached it.
if [[ "$EXIT_CODE" -eq 124 ]]; then
  echo "exploration budget exhausted (${BUDGET_MINUTES}m); treating as controlled stop"
  exit 0
fi

exit "$EXIT_CODE"
