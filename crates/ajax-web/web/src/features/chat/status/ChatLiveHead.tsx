import type { ReactNode } from "react";
import {
  activePlanStep,
  activeTool,
  latestPlan,
  latestThought,
  thoughtSnippet,
  type ChatSessionView,
} from "../session/public";
import LiveHead from "./LiveHead";
import { buildHeadView, headState, type ChatTaskAttention } from "./headView";

interface Props {
  view: ChatSessionView;
  taskAttention: ChatTaskAttention | null;
  activityAgeMs: number;
  connected: boolean;
  permission?: ReactNode;
  actions?: ReactNode;
  onStop: () => void;
}

/** Builds the typed head view and renders LiveHead; permission markup is composed upstream. */
export default function ChatLiveHead({
  view,
  taskAttention,
  activityAgeMs,
  connected,
  permission = null,
  actions = null,
  onStop,
}: Props) {
  const plan = latestPlan(view.conversation);
  const headTool = activeTool(view);
  const headPlanStep = activePlanStep(plan);
  const hasHeadWork = Boolean(headTool || headPlanStep);
  const workingHead =
    headState(
      view.permission.decision,
      view.turn.busy,
      taskAttention,
      view.status.acpState,
    ) === "working";
  const headThought =
    workingHead && !hasHeadWork
      ? (() => {
          const text = latestThought(view.conversation);
          return text ? thoughtSnippet(text) : null;
        })()
      : null;
  const headView = buildHeadView({
    session: view,
    taskAttention,
    tool: headTool,
    planStep: headPlanStep,
    thoughtSnippet: headThought,
    activityAgeMs,
    connected,
  });

  return (
    <LiveHead view={headView} permission={permission} actions={actions} onStop={onStop} />
  );
}
