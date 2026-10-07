import type { ReactNode } from "react";
import { ContextUsageMeter } from "./UsageIndicators";
import { headStateLabel, type ChatHeadView } from "./headView";

interface Props {
  view: ChatHeadView;
  permission?: ReactNode;
  actions?: ReactNode;
  onStop: () => void;
}

export default function LiveHead({ view, permission, actions, onStop }: Props) {
  const quiet = view.state === "working" && view.activityAgeMs >= 60_000;
  const showThinking = view.state === "working" && !view.hasActivity;

  return (
    <section
      className={`session-head tone-${view.tone}`}
      data-testid="session-head"
      data-state={view.state}
    >
      {view.showHeadLine ? (
        <div className="session-head-line" data-testid="session-head-line" aria-live="polite">
          <span
            className={`status-dot${view.state === "working" && !quiet ? " is-live" : ""}`}
            aria-hidden="true"
          />
          {}
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
          {view.usage ? <ContextUsageMeter usage={view.usage} /> : null}
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

      {!view.showHeadLine && view.usage ? <ContextUsageMeter usage={view.usage} /> : null}
    </section>
  );
}
