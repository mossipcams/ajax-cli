import type { ReactNode } from "react";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import { Button } from "@/shared/ui/button";
import type { Decision, ToolCall } from "./projectSession";

export type HeadState = "decision" | "working" | "attention" | "idle";

const STATE_LABELS: Record<HeadState, string> = {
  decision: "Needs you",
  working: "Working",
  attention: "Needs you",
  idle: "Ready",
};

const TOOL_TONES: Record<string, string> = {
  read: "muted",
  edit: "running",
  delete: "error",
  move: "running",
  search: "muted",
  execute: "running",
  think: "muted",
  fetch: "muted",
};

export function headState(
  decision: Decision | null,
  busy: boolean,
  detail: BrowserTaskDetail | null,
): HeadState {
  if (decision) return "decision";
  if (busy) return "working";
  if (detail && (detail.status === "waiting" || detail.status === "error")) return "attention";
  return "idle";
}

export function headTone(state: HeadState, detail: BrowserTaskDetail | null): string {
  if (state === "decision") return "waiting";
  if (state === "working") return "running";
  if (state === "attention") return detail?.status === "error" ? "error" : "waiting";
  return "idle";
}

export function shortPath(path: string): string {
  const parts = path.split("/").filter(Boolean);
  if (parts.length <= 2) return parts.join("/");
  return `…/${parts.slice(-2).join("/")}`;
}

function ToolRow({ call }: { call: ToolCall }) {
  const tone = TOOL_TONES[call.kind] ?? "muted";
  const location = call.locations[0];
  return (
    <div className={`session-tool tone-${tone}`} data-testid="session-head-tool">
      <span className="session-tool-kind">{call.kind || "tool"}</span>
      <span className="session-tool-title">{call.title || call.callId}</span>
      <span className="session-tool-path" title={location || undefined}>
        {location ? shortPath(location) : "\u00a0"}
      </span>
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
  planStep: string | null;
  status: string | null;
  connected: boolean;
  offline: boolean;
  actions?: ReactNode;
  onBack: () => void;
  onApprove: () => void;
  onReject: () => void;
  onStop: () => void;
  onOpenDetails: () => void;
}

export default function LiveHead({
  title,
  state,
  tone,
  detail,
  decision,
  tool,
  planStep,
  status,
  connected,
  offline,
  actions,
  onBack,
  onApprove,
  onReject,
  onStop,
  onOpenDetails,
}: Props) {
  return (
    <section
      className={`session-head tone-${tone}`}
      data-testid="session-head"
      data-state={state}
    >
      <div className="session-head-line" aria-live="polite">
        <button type="button" className="session-head-back" onClick={onBack}>
          ←
        </button>
        <h1 className="session-title">{title}</h1>
        <span
          className={`status-dot${state === "working" ? " is-live" : ""}`}
          aria-hidden="true"
        />
        <span className="session-head-label">{STATE_LABELS[state]}</span>
        {offline ? (
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
          {!tool && !planStep ? (
            <p className="session-head-quiet">{status ?? "Thinking…"}</p>
          ) : null}
        </div>
      ) : null}

      {state === "attention" && detail ? (
        <div className="session-attention">
          <p className="session-head-quiet" data-testid="session-attention">
            {detail.status_explanation?.trim() ||
              (detail.status === "error" ? "The task reported an error." : "Waiting for you.")}
          </p>
          {actions}
        </div>
      ) : null}
    </section>
  );
}
