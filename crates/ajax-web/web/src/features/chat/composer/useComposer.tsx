import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
  type RefObject,
} from "react";
import { autoGrow } from "./autoGrow";
import {
  beginStopAndSend,
  clearQueue,
  composerIsStopping,
  composerQueuedText,
  queueFollowUp,
  restoreQueuedDraft,
  type ComposerState,
} from "./composerState";
import { flushQueuedFollowUp } from "./submit";
import { useChatSpeech } from "./speech/useChatSpeech";

export type ComposerCommands = {
  sendPrompt: (text: string) => boolean;
  sendCancel: () => void;
  markStopped: () => void;
};

export type ComposerProviderProps = ComposerCommands & {
  handle: string;
  connected: boolean;
  busy: boolean;
  everOpened: boolean;
  composerRef: RefObject<HTMLTextAreaElement | null>;
  scrollToLatest: () => void;
  children: ReactNode;
};

type ComposerContextValue = {
  draft: string;
  composerRef: RefObject<HTMLTextAreaElement | null>;
  queued: string | null;
  stopping: boolean;
  submitLabel: string;
  speechModel: ReturnType<typeof useChatSpeech>["speechModel"];
  micAriaLabel: string;
  micArmed: boolean;
  toggleMic: () => void;
  onDraftChange: (value: string, shrank: boolean) => void;
  onKeyDown: (key: string, shiftKey: boolean) => void;
  submitComposer: (event: FormEvent<HTMLFormElement>) => void;
  editQueued: () => void;
  removeQueued: () => void;
  connected: boolean;
  everOpened: boolean;
  busy: boolean;
};

const ComposerContext = createContext<ComposerContextValue | null>(null);

export function useComposerContext(): ComposerContextValue {
  const value = useContext(ComposerContext);
  if (!value) throw new Error("useComposerContext requires ComposerProvider");
  return value;
}

export function ComposerProvider({
  handle,
  connected,
  busy,
  everOpened,
  composerRef,
  scrollToLatest,
  sendPrompt,
  sendCancel,
  markStopped,
  children,
}: ComposerProviderProps) {
  const draftRef = useRef("");
  const [draft, setDraft] = useState("");
  const [composerState, setComposerState] = useState<ComposerState>({ status: "idle" });

  const {
    speechModel,
    micAriaLabel,
    micArmed,
    toggleMic,
  } = useChatSpeech({
    handle,
    draftRef,
    setDraft,
  });

  const queued = composerQueuedText(composerState);
  const stopping = composerIsStopping(composerState);

  const clearDraft = useCallback(() => {
    draftRef.current = "";
    setDraft("");
    if (composerRef.current) composerRef.current.style.height = "";
  }, [composerRef]);

  const sendDraft = useCallback(() => {
    if (!connected) return;
    const text = draftRef.current.trim();
    const queuedText = composerQueuedText(composerState);
    const isStopping = composerIsStopping(composerState);

    if (queuedText !== null) {
      if (text) setComposerState(queueFollowUp(composerState, text));
      clearDraft();
      if (busy && !isStopping) {
        sendCancel();
        setComposerState(beginStopAndSend(composerState));
      }
      scrollToLatest();
      return;
    }

    if (!text) return;
    if (busy) {
      setComposerState(queueFollowUp(composerState, text));
      clearDraft();
      scrollToLatest();
      return;
    }
    if (!sendPrompt(text)) return;
    clearDraft();
    scrollToLatest();
  }, [
    busy,
    clearDraft,
    composerState,
    connected,
    scrollToLatest,
    sendCancel,
    sendPrompt,
  ]);

  useEffect(() => {
    setComposerState((current) =>
      flushQueuedFollowUp({
        composerState: current,
        busy,
        connected,
        sendPrompt,
        markStopped,
      }),
    );
  }, [busy, connected, markStopped, sendPrompt]);

  const editQueued = useCallback(() => {
    const restored = restoreQueuedDraft(composerState);
    if (!restored) return;
    draftRef.current = restored.draft;
    setDraft(restored.draft);
    setComposerState(restored.state);
    composerRef.current?.focus();
  }, [composerRef, composerState]);

  const removeQueued = useCallback(() => {
    setComposerState(clearQueue(composerState));
  }, [composerState]);

  const submitComposer = useCallback(
    (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      sendDraft();
    },
    [sendDraft],
  );

  const onDraftChange = useCallback(
    (value: string, shrank: boolean) => {
      draftRef.current = value;
      if (composerRef.current) autoGrow(composerRef.current, shrank);
      setDraft(value);
    },
    [composerRef],
  );

  const onKeyDown = useCallback(
    (key: string, shiftKey: boolean) => {
      if (key === "Enter" && !shiftKey) sendDraft();
    },
    [sendDraft],
  );

  const submitLabel = queued !== null ? "Stop & send" : busy ? "Queue" : "Send";

  const value: ComposerContextValue = {
    draft,
    composerRef,
    queued,
    stopping,
    submitLabel,
    speechModel,
    micAriaLabel,
    micArmed,
    toggleMic,
    onDraftChange,
    onKeyDown,
    submitComposer,
    editQueued,
    removeQueued,
    connected,
    everOpened,
    busy,
  };

  return <ComposerContext.Provider value={value}>{children}</ComposerContext.Provider>;
}
