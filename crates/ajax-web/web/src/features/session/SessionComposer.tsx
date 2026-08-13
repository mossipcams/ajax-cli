import { useEffect, type FormEvent, type RefObject } from "react";

function autoGrow(node: HTMLTextAreaElement, shrank: boolean) {
  if (shrank) node.style.height = "auto";
  else if (node.scrollHeight <= node.clientHeight) return;
  node.style.height = `${node.scrollHeight}px`;
}

function placeholder(
  connected: boolean,
  everOpened: boolean,
  busy: boolean,
  queuedFollowUp: boolean,
): string {
  if (!connected) return everOpened ? "Reconnecting…" : "Starting…";
  if (busy && queuedFollowUp) return "Enter again to stop and send";
  if (busy) return "Sends after this turn…";
  return "Message…";
}

interface Props {
  composerId: string;
  composerRef: RefObject<HTMLTextAreaElement | null>;
  connected: boolean;
  everOpened: boolean;
  busy: boolean;
  queuedFollowUp: boolean;
  draft: string;
  onDraft: (next: string, node: HTMLTextAreaElement) => void;
  onSubmit: (fromComposer?: HTMLTextAreaElement | null) => void;
}

export default function SessionComposer({
  composerId,
  composerRef,
  connected,
  everOpened,
  busy,
  queuedFollowUp,
  draft,
  onDraft,
  onSubmit,
}: Props) {
  useEffect(() => {
    const node = composerRef.current;
    if (!node) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Enter" || event.shiftKey) return;
      event.preventDefault();
      onSubmit(event.target as HTMLTextAreaElement);
    };
    node.addEventListener("keydown", onKeyDown);
    return () => node.removeEventListener("keydown", onKeyDown);
  }, [composerRef, onSubmit]);

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    onSubmit();
  }

  return (
    <form
      className="session-composer"
      data-testid="session-composer"
      aria-label="Session composer"
      onSubmit={submit}
    >
      <textarea
        id={composerId}
        rows={1}
        enterKeyHint="send"
        placeholder={placeholder(connected, everOpened, busy, queuedFollowUp)}
        aria-label="Message"
        ref={composerRef}
        value={draft}
        onChange={(event) => {
          const next = event.target.value;
          autoGrow(event.currentTarget, next.length < draft.length);
          onDraft(next, event.currentTarget);
        }}
      />
    </form>
  );
}
