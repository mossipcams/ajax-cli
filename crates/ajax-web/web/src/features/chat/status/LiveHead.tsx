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

function HeadToolRow({ tool }: { tool: NonNullable<ChatHeadView["tool"]> }) {
  return (
    <div
      className={`session-tool tone-${tool.tone}`}
      data-testid="session-head-tool"
      data-kind={tool.kind}
    >
      <span className="session-tool-mark" aria-hidden="true">
        {tool.mark}
      </span>
      <span className="session-tool-title">{tool.title}</span>
      {tool.path ? (
        <span className="session-tool-path" title={tool.path}>
          {tool.path}
        </span>
      ) : null}
      <span className="session-row-meta">{tool.statusLabel}</span>
    </div>
  );
}

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
  const hasToolOrPlan = Boolean(view.tool || view.planStep);
  const showThinking =
    view.state === "working" && !hasToolOrPlan && !view.thoughtSnippet;

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
          <span className="session-head-label">{headStateLabel(view.state, quiet)}</span>
          {!view.connected ? (
            <span className="session-head-offline" data-testid="session-head-offline">
              Reconnecting
            </span>
          ) : null}
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
          {view.tool ? <HeadToolRow tool={view.tool} /> : null}
          {view.planStep ? (
            <p className="session-head-quiet" data-testid="session-plan-step">
              {view.planStep}
            </p>
          ) : null}
          {view.thoughtSnippet && !hasToolOrPlan ? (
            <p
              className="session-head-quiet session-head-thought"
              data-testid="session-head-thought"
            >
              {view.thoughtSnippet}
            </p>
          ) : null}
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
