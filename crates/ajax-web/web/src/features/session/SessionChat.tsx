import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { Button } from "@/shared/ui/button";
import { useSessionChat } from "./useSessionChat";

const TaskTerminal = lazy(() => import("@/features/task/TaskTerminal"));

interface Props {
  handle: string;
}

function composerPlaceholder(working: boolean, queuedFollowUp: boolean): string {
  if (working && queuedFollowUp) return "Enter again to stop and send";
  if (working) return "Sends after this turn…";
  return "Message the agent…";
}

export default function SessionChat({ handle }: Props) {
  const {
    connected,
    working,
    workingRef,
    permission,
    sendPrompt,
    sendCancelKeepQueue,
    sendPermission,
  } = useSessionChat(handle);
  const sendPromptRef = useRef(sendPrompt);
  const sendCancelRef = useRef(sendCancelKeepQueue);
  sendPromptRef.current = sendPrompt;
  sendCancelRef.current = sendCancelKeepQueue;

  const [queuedFollowUp, setQueuedFollowUp] = useState(false);
  const queuedFollowUpRef = useRef(false);
  const composerNodeRef = useRef<HTMLTextAreaElement | null>(null);
  const composerCleanupRef = useRef<(() => void) | null>(null);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [showSendButton, setShowSendButton] = useState(() => window.innerWidth > 390);

  useEffect(() => {
    if (!working) {
      queuedFollowUpRef.current = false;
      setQueuedFollowUp(false);
    }
  }, [working]);

  useEffect(() => {
    return () => composerCleanupRef.current?.();
  }, []);

  useEffect(() => {
    const media = window.matchMedia("(min-width: 391px)");
    const sync = () => setShowSendButton(window.innerWidth > 390);
    sync();
    media.addEventListener("change", sync);
    return () => media.removeEventListener("change", sync);
  }, []);

  const submitComposer = useCallback((node?: HTMLTextAreaElement | null) => {
    const field = node ?? composerNodeRef.current;
    if (!field) return;
    const text = field.value.trim();
    if (text) {
      if (!sendPromptRef.current(text)) return;
      field.value = "";
      if (workingRef.current) {
        queuedFollowUpRef.current = true;
        setQueuedFollowUp(true);
      }
      return;
    }
    if (workingRef.current && queuedFollowUpRef.current) {
      sendCancelRef.current();
      queuedFollowUpRef.current = false;
      setQueuedFollowUp(false);
    }
  }, [workingRef]);

  const bindComposer = useCallback(
    (node: HTMLTextAreaElement | null) => {
      composerCleanupRef.current?.();
      composerCleanupRef.current = null;
      composerNodeRef.current = node;
      if (!node) return;
      const onKeyDown = (event: KeyboardEvent) => {
        if (event.key !== "Enter" || event.shiftKey) return;
        event.preventDefault();
        submitComposer(node);
      };
      node.addEventListener("keydown", onKeyDown);
      composerCleanupRef.current = () => node.removeEventListener("keydown", onKeyDown);
    },
    [submitComposer],
  );

  return (
    <section data-testid="session-chat" className="session-chat" data-handle={handle}>
      <header
        data-testid="session-head"
        className="session-head"
        data-state={working ? "working" : "idle"}
      >
        <h2>{handle}</h2>
        {!connected ? (
          <p data-testid="session-head-offline" className="session-head-offline">
            Reconnecting…
          </p>
        ) : null}
      </header>

      {permission ? (
        <div data-testid="session-decision" className="session-decision">
          <p>{permission.title}</p>
          <div className="session-decision-actions">
            <Button type="button" disabled={!connected} onClick={() => sendPermission(true)}>
              Approve
            </Button>
            <Button
              type="button"
              variant="secondary"
              disabled={!connected}
              onClick={() => sendPermission(false)}
            >
              Reject
            </Button>
          </div>
        </div>
      ) : null}

      <div data-testid="session-composer" className="session-composer">
        <textarea
          ref={bindComposer}
          defaultValue=""
          placeholder={composerPlaceholder(working, queuedFollowUp)}
          enterKeyHint="send"
          rows={3}
        />
        {showSendButton ? (
          <Button type="button" onClick={() => submitComposer()}>
            Send
          </Button>
        ) : null}
      </div>

      <Button
        type="button"
        variant="secondary"
        data-testid="session-details"
        onClick={() => setDetailsOpen((open) => !open)}
      >
        Details
      </Button>

      {detailsOpen ? (
        <div className="session-details-panel">
          <Button
            type="button"
            variant="secondary"
            data-testid="session-terminal-toggle"
            onClick={() => setTerminalOpen((open) => !open)}
          >
            Terminal
          </Button>
        </div>
      ) : null}

      {terminalOpen ? (
        <div data-testid="session-terminal-sheet" className="session-terminal-sheet">
          <Suspense fallback={null}>
            <TaskTerminal handle={handle} />
          </Suspense>
        </div>
      ) : null}
    </section>
  );
}
