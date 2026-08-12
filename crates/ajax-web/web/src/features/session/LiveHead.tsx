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
import type { Decision, ToolCall } from "./sessionThread";

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

/** Paths are long and their tail is the informative end, so keep the last two
 * segments rather than ellipsizing the filename away. */
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
  /** In-progress ACP plan step, if any. Not the whole checklist. */
  planStep: string | null;
  status: string | null;
  connected: boolean;
  /** The task's own actions, rendered by the caller so the head stays free of
   * mutation wiring. Shown only in the `attention` state. */
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
      {/* The live region is scoped to the state line and tool row: wrapping the
          whole head made every thought chunk re-announce Stop and Details too. */}
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
          {/* The head must answer "what do I do" precisely when the task needs a
              human. Render the server's own actions, in the server's order —
              `primary_action` is always Resume and must not drive ordering. */}
          {actions}
        </div>
      ) : null}
    </section>
  );
}
