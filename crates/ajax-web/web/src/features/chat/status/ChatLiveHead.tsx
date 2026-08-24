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
  // One bit, not three rows: has this turn produced anything the transcript's
  // activity row can narrate? Until it has, the head says `Thinking…`; after
  // that the transcript owns the operation and the head stays out of it.
  // Scoped to the turn in flight: an earlier turn's tool rows must not silence
  // `Thinking…` for a turn that has not produced anything yet.
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
