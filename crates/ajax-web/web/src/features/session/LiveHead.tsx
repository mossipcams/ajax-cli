// The live head: one fixed panel that always answers "what is the agent doing
// and does it need me?". It escalates — a decision outranks work in progress,
// which outranks a task that needs attention, which outranks idle — so the
// operator reads one state, never four competing banners.
//
// Back + title live here too: a second sticky header stole a full 44px row and
// left the transcript well under the ~80% band this surface needs.

import type { ReactNode } from "react";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import { Button } from "@/shared/ui/button";
import type { Decision, ToolCall, TurnUsage, Usage } from "./sessionThread";
import { cleanTitle, shortPath, toolMark, toolStatusLabel, TOOL_TONES } from "./toolPresentation";

export { shortPath } from "./toolPresentation";

export type HeadState = "decision" | "working" | "attention" | "idle";

const STATE_LABELS: Record<HeadState, string> = {
  decision: "Needs you",
  working: "Working",
  attention: "Needs you",
  idle: "Ready",
};

function agentNeedsYou(status: string | null): boolean {
  const token = status?.trim().toLowerCase();
  return token === "waiting" || token === "requires_action";
}

function agentWorking(status: string | null): boolean {
  return status?.trim().toLowerCase() === "running";
}

export function headState(
  decision: Decision | null,
  busy: boolean,
  detail: BrowserTaskDetail | null,
  agentStatus: string | null,
): HeadState {
  if (decision) return "decision";
  if (agentNeedsYou(agentStatus)) return "attention";
  if (agentWorking(agentStatus) || busy) return "working";
  if (detail && (detail.status === "waiting" || detail.status === "error")) return "attention";
  return "idle";
}

export function headTone(state: HeadState, detail: BrowserTaskDetail | null): string {
  if (state === "decision") return "waiting";
  if (state === "working") return "running";
  if (state === "attention") return detail?.status === "error" ? "error" : "waiting";
  return "idle";
}

/** Context pressure from ACP `usage_update`. Shown whenever the harness reports
 * a non-zero window; high pressure gets a warning tone at 90%+. */
function UsageMeter({ usage }: { usage: Usage }) {
  const ratio = Math.min(1, usage.used / usage.size);
  return (
    <p
      className={`session-head-quiet session-usage${ratio >= 0.9 ? " is-tight" : ""}`}
      data-testid="session-usage"
    >
      Context {Math.round(ratio * 100)}% full
    </p>
  );
}

const TURN_USAGE_FIELDS: { key: keyof TurnUsage; label: string }[] = [
  { key: "inputTokens", label: "input" },
  { key: "outputTokens", label: "output" },
  { key: "cacheReadTokens", label: "cache read" },
  { key: "cacheWriteTokens", label: "cache write" },
  { key: "totalTokens", label: "total" },
];

/** Per-turn token counts from ACP prompt results. Only present fields are
 * shown — missing counts are omitted, never rendered as zero. */
export function formatTurnUsage(turnUsage: TurnUsage): string | null {
  const parts = TURN_USAGE_FIELDS.flatMap(({ key, label }) => {
    const value = turnUsage[key];
    if (typeof value !== "number") return [];
    return [`${label} ${value.toLocaleString()}`];
  });
  if (parts.length === 0) return null;
  return `Turn tokens: ${parts.join(" · ")}`;
}

function ToolRow({ call }: { call: ToolCall }) {
  const tone = TOOL_TONES[call.kind] ?? "muted";
  const location = call.locations[0];
  return (
    <div
      className={`session-tool tone-${tone}`}
      data-testid="session-head-tool"
      data-kind={call.kind || "other"}
    >
      <span className="session-tool-mark" aria-hidden="true">
        {toolMark(call.kind)}
      </span>
      <span className="session-tool-title">{cleanTitle(call.title) || call.callId}</span>
      {location ? (
        <span className="session-tool-path" title={location}>
          {shortPath(location)}
        </span>
      ) : null}
      <span className="session-row-meta">{toolStatusLabel(call.status)}</span>
    </div>
  );
}

interface Props {
  title: string;
  state: HeadState;
  tone: string;
  detail: BrowserTaskDetail | null;
  decision: Decision | null;
  tool: ToolCall | null;
  /** In-progress ACP plan step, if any. Not the whole checklist. */
  planStep: string | null;
  /** One-line latest ACP thought while working. */
  thoughtSnippet: string | null;
  /** Latest context pressure, or null when the harness does not report it. */
  usage: Usage | null;
  /** Latest per-turn token usage, or null when the harness does not report it. */
  turnUsage: TurnUsage | null;
  activityAgeMs: number;
  connected: boolean;
  /** The task's own actions, rendered by the caller so the head stays free of
   * mutation wiring. Shown only in the `attention` state. */
  actions?: ReactNode;
  onBack: () => void;
  onApprove: () => void;
  onReject: () => void;
  onStop: () => void;
  onOpenDetails: () => void;
  /** When the task details sheet is open — drives Details disclosure a11y. */
  detailsOpen?: boolean;
  /** Panel id for `aria-controls` on the Details control. */
  detailsPanelId?: string;
}

export default function LiveHead({
  title,
  state,
  tone,
  detail,
  decision,
  tool,
  planStep,
  thoughtSnippet,
  usage,
  turnUsage: _turnUsage,
  activityAgeMs,
  connected,
  actions,
  onBack,
  onApprove,
  onReject,
  onStop,
  onOpenDetails,
  detailsOpen = false,
  detailsPanelId,
}: Props) {
  const quiet = state === "working" && activityAgeMs >= 60_000;
  const hasToolOrPlan = Boolean(tool || planStep);
  const showThinking = state === "working" && !hasToolOrPlan && !thoughtSnippet;
  return (
    <section
      className={`session-head tone-${tone}`}
      data-testid="session-head"
      data-state={state}
    >
      {/* The live region is scoped to the state line and tool row: wrapping the
          whole head made every thought chunk re-announce Stop and Details too. */}
      <div className="session-head-line" aria-live="polite">
        <button type="button" className="session-head-back" onClick={onBack}>
          ←
        </button>
        <h1 className="session-title">{title}</h1>
        <span
          className={`status-dot${state === "working" && !quiet ? " is-live" : ""}`}
          aria-hidden="true"
        />
        <span className="session-head-label">
          {quiet ? "No recent activity" : STATE_LABELS[state]}
        </span>
        {!connected ? (
          <span className="session-head-offline" data-testid="session-head-offline">
            Reconnecting
          </span>
        ) : null}
        <div className="session-head-controls">
          {state === "working" ? (
            <button
              type="button"
              className="session-head-stop"
              data-testid="session-cancel"
              onClick={onStop}
            >
              Stop
            </button>
          ) : null}
          <button
            type="button"
            className="session-head-details"
            data-testid="session-details"
            aria-expanded={detailsOpen}
            {...(detailsPanelId ? { "aria-controls": detailsPanelId } : {})}
            onClick={onOpenDetails}
          >
            Details
          </button>
        </div>
      </div>

      {state === "decision" && decision ? (
        <div className="session-decision" data-testid="session-decision" role="alert">
          <p className="session-decision-title">{decision.title}</p>
          {decision.detail ? (
            <p className="session-decision-detail">{decision.detail}</p>
          ) : null}
          {/* Disabled while the socket is down: the handler already refuses to
              answer on a dead connection, so an enabled-looking control would
              be a silent no-op. The `Reconnecting` flag above says why. */}
          <div className="session-decision-actions">
            <Button type="button" variant="default" disabled={!connected} onClick={onApprove}>
              Approve
            </Button>
            <Button type="button" variant="secondary" disabled={!connected} onClick={onReject}>
              Reject
            </Button>
          </div>
        </div>
      ) : null}

      {state === "working" ? (
        <div className="session-working" aria-live="polite">
          {tool ? <ToolRow call={tool} /> : null}
          {planStep ? (
            <p className="session-head-quiet" data-testid="session-plan-step">
              {planStep}
            </p>
          ) : null}
          {thoughtSnippet && !hasToolOrPlan ? (
            <p
              className="session-head-quiet session-head-thought"
              data-testid="session-head-thought"
            >
              {thoughtSnippet}
            </p>
          ) : null}
          {showThinking ? (
            <p className="session-head-quiet" data-testid="session-head-idle">
              Thinking…
            </p>
          ) : null}
          {quiet ? (
            <p className="session-head-quiet" data-testid="session-head-activity-age">
              Last update {Math.max(1, Math.floor(activityAgeMs / 60_000))}m ago
            </p>
          ) : null}
        </div>
      ) : null}

      {state === "attention" && detail ? (
        <div className="session-attention">
          <p className="session-head-quiet" data-testid="session-attention">
            {detail.status_explanation?.trim() ||
              (detail.status === "error" ? "The task reported an error." : "Waiting for you.")}
          </p>
          {/* The head must answer "what do I do" precisely when the task needs a
              human. Render the server's own actions, in the server's order —
              `primary_action` is always Resume and must not drive ordering. */}
          {actions}
        </div>
      ) : null}

      {usage ? <UsageMeter usage={usage} /> : null}
    </section>
  );
}
