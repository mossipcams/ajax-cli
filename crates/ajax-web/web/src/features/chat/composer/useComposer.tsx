import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type ClipboardEvent,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
} from "react";
import type { LiveAvailableCommand } from "@/shared/lib/liveSessionCommands";
import type { LivePromptCapabilities } from "@/shared/lib/liveSessionPromptCapabilities";
import type { ComposerAttachment, PromptContentBlockWire } from "@/shared/lib/promptContent";
import {
  attachmentFromFile,
  attachmentFromPaste,
  attachmentsFromContentBlocks,
  canAttachFiles,
  fitPromptContentBlocks,
  flattenAttachmentBlocks,
  promptFrameFits,
} from "@/shared/lib/promptContent";
import { autoGrow } from "./autoGrow";
import {
  beginStopAndSend,
  clearQueue,
  composerIsStopping,
  composerQueuedContentBlocks,
  composerQueuedText,
  queueFollowUp,
  restoreQueuedDraft,
  type ComposerState,
} from "./composerState";
import {
  composerStateAfterFlush,
  flushQueuedFollowUp,
} from "./submit";
import { useChatSpeech } from "./speech/useChatSpeech";
import {
  filterAdvertisedCommands,
  insertSlashCommand,
  parseSlashPrefix,
} from "./slashCompletion";

export type ComposerCommands = {
  sendPrompt: (text: string, contentBlocks?: PromptContentBlockWire[]) => boolean;
  sendCancel: () => void;
  markStopped: () => void;
};

export type ComposerProviderProps = ComposerCommands & {
  handle: string;
  connected: boolean;
  busy: boolean;
  everOpened: boolean;
  availableCommands?: LiveAvailableCommand[];
  promptCapabilities?: LivePromptCapabilities;
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
  onComposerKeyDown: (event: KeyboardEvent<HTMLTextAreaElement>) => void;
  submitComposer: (event: FormEvent<HTMLFormElement>) => void;
  editQueued: () => void;
  removeQueued: () => void;
  connected: boolean;
  everOpened: boolean;
  busy: boolean;
  slashMatches: LiveAvailableCommand[];
  slashMenuOpen: boolean;
  slashSelection: number;
  insertSlashMatch: (command: LiveAvailableCommand) => void;
  attachments: ComposerAttachment[];
  removeAttachment: (id: string) => void;
  attachFiles: (files: FileList | File[]) => Promise<void>;
  onComposerPaste: (event: React.ClipboardEvent<HTMLTextAreaElement>) => void;
  attachInputRef: RefObject<HTMLInputElement | null>;
  attachAccept: string;
  canAttach: boolean;
  attachmentError: string | null;
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
  availableCommands,
  promptCapabilities,
  composerRef,
  scrollToLatest,
  sendPrompt,
  sendCancel,
  markStopped,
  children,
}: ComposerProviderProps) {
  const draftRef = useRef("");
  const composerStateRef = useRef<ComposerState>({ status: "idle" });
  const [draft, setDraft] = useState("");
  const [composerState, setComposerState] = useState<ComposerState>({ status: "idle" });
  const [slashSelection, setSlashSelection] = useState(0);
  const [attachments, setAttachments] = useState<ComposerAttachment[]>([]);
  const [attachmentError, setAttachmentError] = useState<string | null>(null);
  const attachInputRef = useRef<HTMLInputElement | null>(null);
  composerStateRef.current = composerState;

  const slashPrefix = useMemo(() => parseSlashPrefix(draft), [draft]);
  const slashMatches = useMemo(
    () => filterAdvertisedCommands(availableCommands, slashPrefix?.prefix ?? ""),
    [availableCommands, slashPrefix],
  );
  const slashMenuOpen = slashPrefix !== null && slashMatches.length > 0;

  useEffect(() => {
    setSlashSelection(0);
  }, [draft, availableCommands]);

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

  const clearDraftText = useCallback(() => {
    draftRef.current = "";
    setDraft("");
    if (composerRef.current) composerRef.current.style.height = "";
  }, [composerRef]);

  const clearDraftAttachments = useCallback(() => {
    setAttachments([]);
  }, []);

  const clearDraft = useCallback(() => {
    clearDraftText();
    clearDraftAttachments();
  }, [clearDraftAttachments, clearDraftText]);

  const contentBlocks = useMemo(
    () => flattenAttachmentBlocks(attachments),
    [attachments],
  );

  const attachAccept = useMemo(() => {
    const parts: string[] = [];
    if (promptCapabilities?.image) parts.push("image/*");
    if (promptCapabilities?.embeddedContext) parts.push("*/*");
    return parts.join(",");
  }, [promptCapabilities?.embeddedContext, promptCapabilities?.image]);

  const canAttach = useMemo(() => canAttachFiles(promptCapabilities), [promptCapabilities]);

  const deliverPrompt = useCallback(
    (promptText: string, blocks: PromptContentBlockWire[]) => {
      if (!sendPrompt(promptText, blocks)) return false;
      clearDraft();
      scrollToLatest();
      return true;
    },
    [clearDraft, scrollToLatest, sendPrompt],
  );

  const sendDraft = useCallback(() => {
    if (!connected) return;
    const text = draftRef.current.trim();
    const queuedText = composerQueuedText(composerState);
    const isStopping = composerIsStopping(composerState);

    if (queuedText !== null) {
      if (text) {
        setComposerState(
          queueFollowUp(composerState, text, composerQueuedContentBlocks(composerState)),
        );
      }
      clearDraftText();
      clearDraftAttachments();
      if (busy && !isStopping) {
        sendCancel();
        setComposerState(beginStopAndSend(composerState));
      }
      scrollToLatest();
      return;
    }

    if (!text) return;
    if (busy) {
      setComposerState(
        queueFollowUp(composerState, text, contentBlocks.length ? contentBlocks : undefined),
      );
      clearDraftText();
      clearDraftAttachments();
      scrollToLatest();
      return;
    }

    if (promptFrameFits(text, contentBlocks)) {
      setAttachmentError(null);
      deliverPrompt(text, contentBlocks);
      return;
    }

    void (async () => {
      const fitted = await fitPromptContentBlocks(text, contentBlocks);
      if (fitted.error) {
        setAttachmentError(fitted.error);
        return;
      }
      setAttachmentError(null);
      deliverPrompt(text, fitted.blocks);
    })();
  }, [
    busy,
    clearDraftAttachments,
    clearDraftText,
    composerState,
    connected,
    contentBlocks,
    deliverPrompt,
    scrollToLatest,
    sendCancel,
  ]);

  useEffect(() => {
    const { intents } = flushQueuedFollowUp({
      composerState: composerStateRef.current,
      busy,
      connected,
    });
    if (intents.length === 0) return;

    let sendSucceeded = false;
    for (const intent of intents) {
      if (intent.type === "mark_stopped") markStopped();
      if (intent.type === "send_prompt") {
        const blocks = intent.contentBlocks ?? [];
        if (promptFrameFits(intent.text, blocks)) {
          setAttachmentError(null);
          sendSucceeded = sendPrompt(intent.text, blocks);
          continue;
        }
        void (async () => {
          const fitted = await fitPromptContentBlocks(intent.text, blocks);
          if (fitted.error) {
            setAttachmentError(fitted.error);
            return;
          }
          setAttachmentError(null);
          if (sendPrompt(intent.text, fitted.blocks)) {
            setComposerState((current) => composerStateAfterFlush(current, true));
          }
        })();
      }
    }
    if (sendSucceeded) {
      setComposerState((current) => composerStateAfterFlush(current, true));
    }
  }, [busy, connected, markStopped, sendPrompt]);

  const editQueued = useCallback(() => {
    const restored = restoreQueuedDraft(composerState);
    if (!restored) return;
    draftRef.current = restored.draft;
    setDraft(restored.draft);
    setComposerState(restored.state);
    setAttachments(
      restored.contentBlocks?.length
        ? attachmentsFromContentBlocks(restored.contentBlocks)
        : [],
    );
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

  const applyDraft = useCallback(
    (value: string, shrank: boolean) => {
      draftRef.current = value;
      if (composerRef.current) autoGrow(composerRef.current, shrank);
      setDraft(value);
    },
    [composerRef],
  );

  const onDraftChange = useCallback(
    (value: string, shrank: boolean) => {
      applyDraft(value, shrank);
    },
    [applyDraft],
  );

  const insertSlashMatch = useCallback(
    (command: LiveAvailableCommand) => {
      applyDraft(insertSlashCommand(command), false);
      composerRef.current?.focus();
    },
    [applyDraft, composerRef],
  );

  const removeAttachment = useCallback((id: string) => {
    setAttachments((current) => current.filter((attachment) => attachment.id !== id));
  }, []);

  const attachFiles = useCallback(
    async (files: FileList | File[]) => {
      const next: ComposerAttachment[] = [];
      for (const file of Array.from(files)) {
        const attachment = await attachmentFromFile(file, promptCapabilities);
        if (attachment) next.push(attachment);
      }
      if (next.length) {
        setAttachments((current) => [...current, ...next]);
      }
    },
    [promptCapabilities],
  );

  const onComposerPaste = useCallback(
    async (event: ClipboardEvent<HTMLTextAreaElement>) => {
      const items = event.clipboardData?.items;
      if (!items?.length) return;
      const next: ComposerAttachment[] = [];
      for (const item of Array.from(items)) {
        const attachment = await attachmentFromPaste(item, promptCapabilities);
        if (attachment) next.push(attachment);
      }
      if (!next.length) return;
      event.preventDefault();
      setAttachments((current) => [...current, ...next]);
    },
    [promptCapabilities],
  );

  const onComposerKeyDown = useCallback(
    (event: KeyboardEvent<HTMLTextAreaElement>) => {
      if (slashMenuOpen) {
        if (event.key === "Tab" || event.key === "Enter") {
          event.preventDefault();
          const selected = slashMatches[slashSelection] ?? slashMatches[0];
          if (selected) insertSlashMatch(selected);
          return;
        }
        if (event.key === "ArrowDown") {
          event.preventDefault();
          setSlashSelection((current) => (current + 1) % slashMatches.length);
          return;
        }
        if (event.key === "ArrowUp") {
          event.preventDefault();
          setSlashSelection(
            (current) => (current - 1 + slashMatches.length) % slashMatches.length,
          );
          return;
        }
      }
      if (event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        sendDraft();
      }
    },
    [insertSlashMatch, sendDraft, slashMatches, slashMenuOpen, slashSelection],
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
    onComposerKeyDown,
    submitComposer,
    editQueued,
    removeQueued,
    connected,
    everOpened,
    busy,
    slashMatches,
    slashMenuOpen,
    slashSelection,
    insertSlashMatch,
    attachments,
    removeAttachment,
    attachFiles,
    onComposerPaste,
    attachInputRef,
    attachAccept,
    canAttach,
    attachmentError,
  };

  return <ComposerContext.Provider value={value}>{children}</ComposerContext.Provider>;
}
