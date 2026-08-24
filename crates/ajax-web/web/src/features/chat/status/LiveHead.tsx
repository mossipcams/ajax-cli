// The live head: one fixed panel that always answers "what is the agent doing
// and does it need me?". It escalates — a decision outranks work in progress,
// which outranks a task that needs attention, which outranks idle — so the
// operator reads one state, never four competing banners.
//
// Identity chrome (back, title, Details) lives on the shared Task Workspace
// header above this panel. Permission controls are composed via `permission`.

import type { ReactNode } from "react";
import { ContextUsageMeter } from "./UsageIndicators";
import { headStateLabel, type ChatHeadView } from "./headView";

interface Props {
  view: ChatHeadView;
  /** Permission markup from features/chat/permissions — not owned by status. */
  permission?: ReactNode;
  /** Task actions for the attention state — composed by Task Workspace. */
  actions?: ReactNode;
  onStop: () => void;
}

export default function LiveHead({ view, permission, actions, onStop }: Props) {
  const quiet = view.state === "working" && view.activityAgeMs >= 60_000;
  // The turn's activity row narrates the operation in the transcript, where the
  // conversation is. The head printing the same command a screen away gave the
  // operator two live regions and a void between them. The head keeps the state
  // and Stop, and speaks only before the first event, when the transcript has
  // nothing to show yet.
  const showThinking = view.state === "working" && !view.hasActivity;

  return (
    <section
      className={`session-head tone-${view.tone}`}
      data-testid="session-head"
      data-state={view.state}
    >
      {view.showHeadLine ? (
        <div className="session-head-line" aria-live="polite">
          <span
            className={`status-dot${view.state === "working" && !quiet ? " is-live" : ""}`}
            aria-hidden="true"
          />
          {/* #1039: one badge. A dropped socket is the state — `Ready` beside
              `Reconnecting` claimed both at once, and neither the agent's
              readiness nor an ask can be acted on until the socket is back. */}
          {view.connected ? (
            <span className="session-head-label">{headStateLabel(view.state, quiet)}</span>
          ) : (
            <span
              className="session-head-label session-head-offline"
              data-testid="session-head-offline"
            >
              Reconnecting
            </span>
          )}
          <div className="session-head-controls">
            {view.state === "working" ? (
              <button
                type="button"
                className="session-head-stop"
                data-testid="session-cancel"
                onClick={onStop}
              >
                Stop
              </button>
            ) : null}
          </div>
        </div>
      ) : null}

      {view.state === "decision" && permission ? permission : null}

      {view.state === "working" ? (
        <div className="session-working" aria-live="polite">
          {showThinking ? (
            <p className="session-head-quiet" data-testid="session-head-idle">
              Thinking…
            </p>
          ) : null}
          {quiet ? (
            <p className="session-head-quiet" data-testid="session-head-activity-age">
              Last update {Math.max(1, Math.floor(view.activityAgeMs / 60_000))}m ago
            </p>
          ) : null}
        </div>
      ) : null}

      {view.state === "attention" && view.taskAttention && view.attentionText ? (
        <div className="session-attention">
          <p className="session-head-quiet" data-testid="session-attention">
            {view.attentionText}
          </p>
          {actions}
        </div>
      ) : null}

      {view.usage ? <ContextUsageMeter usage={view.usage} /> : null}
    </section>
  );
}
