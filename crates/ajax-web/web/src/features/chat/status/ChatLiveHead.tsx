import type { ReactNode } from "react";
import type { ChatSessionView } from "../session/public";
import LiveHead from "./LiveHead";
import { buildHeadView, type ChatTaskAttention } from "./headView";

interface Props {
  view: ChatSessionView;
  taskAttention: ChatTaskAttention | null;
  activityAgeMs: number;
  connected: boolean;
  permission?: ReactNode;
  actions?: ReactNode;
  onStop: () => void;
}

export default function ChatLiveHead({
  view,
  taskAttention,
  activityAgeMs,
  connected,
  permission = null,
  actions = null,
  onStop,
}: Props) {
  const hasActivity = (() => {
    for (let i = view.conversation.length - 1; i >= 0; i -= 1) {
      const item = view.conversation[i];
      if (item.kind === "prose" && item.role === "user") return false;
      if (item.kind === "tool" || item.kind === "plan" || item.kind === "thought") return true;
    }
    return false;
  })();
  const headView = buildHeadView({
    session: view,
    taskAttention,
    hasActivity,
    activityAgeMs,
    connected,
  });

  return (
    <LiveHead view={headView} permission={permission} actions={actions} onStop={onStop} />
  );
}
