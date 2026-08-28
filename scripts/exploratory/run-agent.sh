#!/usr/bin/env bash
# Launch Cursor Agent for exploratory testing with a hard time budget.
# Uses exploratory MCP + CLI permission overlays under $HOME/.cursor.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPTS="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS="$ROOT/exploratory-results"
PROMPT_FILE="$RESULTS/prompt.txt"
AGENT_LOG="$RESULTS/logs/agent.log"
BUDGET_MINUTES="${AJAX_EXPLORATORY_BUDGET_MINUTES:-12}"
FINALIZATION_RESERVE_MINUTES="${AJAX_EXPLORATORY_FINALIZATION_MINUTES:-2}"
EXPLORATION_MINUTES=$((BUDGET_MINUTES - FINALIZATION_RESERVE_MINUTES))
if [ "$EXPLORATION_MINUTES" -lt 1 ]; then
  EXPLORATION_MINUTES=1
fi
AGENT_TIMEOUT_SECONDS=$((EXPLORATION_MINUTES * 60))
MODEL="composer-2.5"

mkdir -p "$RESULTS/logs" "$HOME/.cursor"
cd "$SCRIPTS"

write_agent_status() {
  local exit_code="$1"
  local error_message="${2:-}"
  EXIT_CODE="$exit_code" ERROR_MESSAGE="$error_message" MODEL="$MODEL" BUDGET_MINUTES="$BUDGET_MINUTES" FINALIZATION_RESERVE_MINUTES="$FINALIZATION_RESERVE_MINUTES" AGENT_TIMEOUT_SECONDS="$AGENT_TIMEOUT_SECONDS" EXPLORATION_MINUTES="$EXPLORATION_MINUTES" \
    node --input-type=module <<'EOF'
import { join } from "node:path";
import { computeAgentBudget, readJson, resultsDir, writeJson } from "./lib.mjs";

const exitCode = Number(process.env.EXIT_CODE);
const errorMessage = process.env.ERROR_MESSAGE || null;
const timedOut = exitCode === 124;
const run = readJson(join(resultsDir, "run.json"), {});
const budget = computeAgentBudget({
  budgetMinutes: Number(process.env.BUDGET_MINUTES),
  finalizationReserveMinutes: Number(process.env.FINALIZATION_RESERVE_MINUTES ?? 2),
});

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
  budgetMinutes: budget.budgetMinutes,
  finalizationReserveMinutes: budget.finalizationReserveMinutes,
  explorationMinutes: budget.explorationMinutes,
  agentTimeoutSeconds: budget.agentTimeoutSeconds,
  error: errorMessage,
  attempts: 1,
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

if ! node "$SCRIPTS/assert-webkit.mjs" >>"$AGENT_LOG" 2>&1; then
  echo "::error title=webkit unavailable::exploratory MCP is not WebKit-only or WebKit is unavailable" >&2
  write_agent_status 1 "webkit unavailable"
  exit 1
fi

MAX_ATTEMPTS=1

set +e
timeout --signal=TERM --kill-after=60s "${AGENT_TIMEOUT_SECONDS}s" \
  "$AGENT_BIN" \
  --print \
  --trust \
  --approve-mcps \
  --force \
  --model "$MODEL" \
  --output-format text \
  --workspace "$ROOT" \
  "$(cat "$PROMPT_FILE")" \
  >>"$AGENT_LOG" 2>&1
EXIT_CODE=$?
set -e

write_agent_status "$EXIT_CODE"

if [[ "$EXIT_CODE" -eq 124 ]]; then
  echo "exploration budget exhausted (${EXPLORATION_MINUTES}m agent runtime; ${FINALIZATION_RESERVE_MINUTES}m finalization reserve held back from timeout); treating as controlled stop"
  exit 0
fi

exit "$EXIT_CODE"
