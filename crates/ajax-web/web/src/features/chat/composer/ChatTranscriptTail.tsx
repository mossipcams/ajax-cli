import type { ReactNode } from "react";
import QueuedFollowUp from "./QueuedFollowUp";
import { useComposerContext } from "./useComposer";

interface Props {
  itemCount: number;
  conversation: ReactNode;
}

export default function ChatTranscriptTail({ itemCount, conversation }: Props) {
  const { queued } = useComposerContext();
  if (itemCount === 0 && queued === null) {
    return (
      <p className="session-thread-empty" data-testid="session-thread-empty">
        Message the agent to steer this task.
      </p>
    );
  }
  return (
    <>
      {conversation}
      <QueuedFollowUp />
    </>
  );
}
