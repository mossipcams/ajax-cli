import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
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
  clearComposerDraft,
  readComposerDraft,
  readComposerQueue,
  writeComposerDraft,
  writeComposerQueue,
} from "./draftStorage";
import {
  clearQueue,
  composerIsStopping,
  composerQueuedContentBlocks,
  composerQueuedText,
  restoreQueuedDraft,
  type ComposerState,
} from "./composerState";
import {
  applySubmitResult,
  composerQueuedClientMessageId,
  flushQueuedFollowUp,
  submitComposerDraft,
} from "./submit";
import { useChatSpeech } from "./speech/useChatSpeech";
import {
  filterAdvertisedCommands,
  insertSlashCommand,
  parseSlashPrefix,
} from "./slashCompletion";
import {
  isBuiltInClearCommand,
  mergeSlashCommands,
} from "./builtInCommands";
import { failedTurnPromptToRestore } from "../session/public";
import type { ConversationItem } from "../session/public";

export type ComposerCommands = {
  sendPrompt: (
    text: string,
    contentBlocks?: PromptContentBlockWire[],
    clientMessageId?: string,
  ) => string;
  withdrawQueuedPrompt: (clientMessageId: string) => void;
  sendCancel: () => void;
  sendClear: () => void;
  markStopped: () => void;
};

export type ComposerProviderProps = ComposerCommands & {
  handle: string;
  connected: boolean;
  busy: boolean;
  everOpened: boolean;
  conversation: ConversationItem[];
  conversationRevision: number;
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
  /** Bumps when draft text is restored from storage (not typed). */
  draftRestoreGeneration: number;
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
  conversation,
  conversationRevision,
  availableCommands,
  promptCapabilities,
  composerRef,
  scrollToLatest,
  sendPrompt,
  withdrawQueuedPrompt,
  sendCancel,
  sendClear,
  markStopped,
  children,
}: ComposerProviderProps) {
  const initialDraft = readComposerDraft(handle);
  const initialQueue = readComposerQueue(handle);
  const draftRef = useRef(initialDraft);
  const composerStateRef = useRef<ComposerState>(initialQueue);
  const [draft, setDraft] = useState(initialDraft);
  const [draftRestoreGeneration, setDraftRestoreGeneration] = useState(() =>
    initialDraft ? 1 : 0,
  );
  const [composerState, setComposerState] = useState<ComposerState>(initialQueue);
  const [slashSelection, setSlashSelection] = useState(0);
  const [attachments, setAttachments] = useState<ComposerAttachment[]>([]);
  const [attachmentError, setAttachmentError] = useState<string | null>(null);
  const attachInputRef = useRef<HTMLInputElement | null>(null);
  const holdRestoredQueueRef = useRef(initialQueue.status !== "idle");
  const handledFailedTurnKeyRef = useRef<string | null>(null);
  const [restoreIdleCheck, setRestoreIdleCheck] = useState(0);
  composerStateRef.current = composerState;
  const busyRef = useRef(busy);
  busyRef.current = busy;

  const slashPrefix = useMemo(() => parseSlashPrefix(draft), [draft]);
  const slashCommandSource = useMemo(
    () => mergeSlashCommands(availableCommands),
    [availableCommands],
  );
  const slashMatches = useMemo(
    () => filterAdvertisedCommands(slashCommandSource, slashPrefix?.prefix ?? ""),
    [slashCommandSource, slashPrefix],
  );
  const slashMenuOpen = slashPrefix !== null && slashMatches.length > 0;

  useEffect(() => {
    setSlashSelection(0);
  }, [draft, availableCommands]);

  useLayoutEffect(() => {
    const stored = readComposerDraft(handle);
    draftRef.current = stored;
    setDraft(stored);
    if (stored) setDraftRestoreGeneration((generation) => generation + 1);
    setComposerState(readComposerQueue(handle));
    holdRestoredQueueRef.current = readComposerQueue(handle).status !== "idle";
    handledFailedTurnKeyRef.current = null;
  }, [handle]);

  useEffect(() => {
    writeComposerQueue(handle, composerState);
  }, [composerState, handle]);

  const persistDraft = useCallback(
    (value: string) => {
      writeComposerDraft(handle, value);
    },
    [handle],
  );

  useEffect(() => {
    if (busy) return;
    const candidate = failedTurnPromptToRestore(conversation);
    if (!candidate) return;
    if (handledFailedTurnKeyRef.current === candidate.failureKey) return;
    handledFailedTurnKeyRef.current = candidate.failureKey;
    if (draftRef.current.trim()) return;
    draftRef.current = candidate.promptText;
    persistDraft(candidate.promptText);
    setDraft(candidate.promptText);
    setDraftRestoreGeneration((generation) => generation + 1);
  }, [busy, conversation, conversationRevision, persistDraft]);

  const setDraftWithPersist = useCallback(
    (value: string) => {
      draftRef.current = value;
      persistDraft(value);
      setDraft(value);
    },
    [persistDraft],
  );

  const {
    speechModel,
    micAriaLabel,
    micArmed,
    toggleMic,
  } = useChatSpeech({
    handle,
    draftRef,
    setDraft: setDraftWithPersist,
  });

  const queued = composerQueuedText(composerState);
  const stopping = composerIsStopping(composerState);

  const clearDraftText = useCallback(() => {
    draftRef.current = "";
    clearComposerDraft(handle);
    setDraft("");
    if (composerRef.current) composerRef.current.style.height = "";
  }, [composerRef, handle]);

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
    (promptText: string, blocks: PromptContentBlockWire[], existingClientMessageId?: string) => {
      const clientMessageId = sendPrompt(promptText, blocks, existingClientMessageId);
      if (!clientMessageId) return "";
      clearDraft();
      scrollToLatest();
      return clientMessageId;
    },
    [clearDraft, scrollToLatest, sendPrompt],
  );


  const sendDraft = useCallback(() => {
    if (!connected) return;

    if (isBuiltInClearCommand(draftRef.current)) {
      sendClear();
      clearDraftText();
      clearDraftAttachments();
      setComposerState(clearQueue(composerStateRef.current));
      scrollToLatest();
      return;
    }

    const result = submitComposerDraft({
      connected,
      busy,
      draft: draftRef.current,
      composerState,
      contentBlocks,
    });

    if (result.action === "none") return;

    if (result.action === "send") {
      const text = result.text;
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
      return;
    }

    if (result.action === "queue" || result.action === "update_queue") {
      const text = result.text;
      const blocks =
        result.action === "update_queue"
          ? (contentBlocks.length
              ? contentBlocks
              : (composerQueuedContentBlocks(composerState) ?? []))
          : contentBlocks;
      const priorId =
        result.action === "update_queue"
          ? composerQueuedClientMessageId(composerState)
          : undefined;
      const applyQueued = (id: string) => {
        if (!id) return;
        setComposerState((current) =>
          applySubmitResult(
            result,
            current,
            {
              connected,
              busy,
              draft: draftRef.current,
              composerState: current,
              contentBlocks: blocks,
            },
            id,
          ),
        );
      };
      if (promptFrameFits(text, blocks)) {
        setAttachmentError(null);
        applyQueued(deliverPrompt(text, blocks, priorId));
        scrollToLatest();
        return;
      }
      void (async () => {
        const fitted = await fitPromptContentBlocks(text, blocks);
        if (fitted.error) {
          setAttachmentError(fitted.error);
          return;
        }
        setAttachmentError(null);
        applyQueued(deliverPrompt(text, fitted.blocks, priorId));
      })();
      scrollToLatest();
      return;
    }

    if ("clearDraft" in result && result.clearDraft) {
      clearDraftText();
      clearDraftAttachments();
    }

    if (
      result.action === "stop_and_send" &&
      busy &&
      !composerIsStopping(composerStateRef.current)
    ) {
      sendCancel();
    }

    setComposerState((current) =>
      applySubmitResult(result, current, {
        connected,
        busy,
        draft: draftRef.current,
        composerState: current,
        contentBlocks,
      }),
    );

    if (
      result.action === "scroll" ||
      result.action === "queue" ||
      result.action === "update_queue" ||
      result.action === "stop_and_send"
    ) {
      scrollToLatest();
    }
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
    sendClear,
  ]);

  useEffect(() => {
    const { intents, state } = flushQueuedFollowUp({
      composerState: composerStateRef.current,
      busy,
      connected,
    });
    if (intents.length === 0 && state === composerStateRef.current) return;

    if (holdRestoredQueueRef.current) {
      if (!connected || !everOpened) return;
      if (busy) {
        holdRestoredQueueRef.current = false;
        return;
      }
      if (restoreIdleCheck === 0) {
        requestAnimationFrame(() => {
          if (!holdRestoredQueueRef.current) return;
          if (busyRef.current) {
            holdRestoredQueueRef.current = false;
            return;
          }
          holdRestoredQueueRef.current = false;
          setRestoreIdleCheck((tick) => tick + 1);
        });
        return;
      }
      holdRestoredQueueRef.current = false;
    }

    for (const intent of intents) {
      if (intent.type === "mark_stopped") markStopped();
    }
    setComposerState(state);
  }, [busy, connected, everOpened, markStopped, restoreIdleCheck]);

  const editQueued = useCallback(() => {
    const priorId = composerQueuedClientMessageId(composerState);
    const restored = restoreQueuedDraft(composerState);
    if (!restored) return;
    if (priorId) withdrawQueuedPrompt(priorId);
    draftRef.current = restored.draft;
    persistDraft(restored.draft);
    setDraft(restored.draft);
    if (restored.draft) setDraftRestoreGeneration((generation) => generation + 1);
    setComposerState(restored.state);
    setAttachments(
      restored.contentBlocks?.length
        ? attachmentsFromContentBlocks(restored.contentBlocks)
        : [],
    );
    composerRef.current?.focus();
  }, [composerRef, composerState, persistDraft, withdrawQueuedPrompt]);

  const removeQueued = useCallback(() => {
    const priorId = composerQueuedClientMessageId(composerState);
    if (priorId) withdrawQueuedPrompt(priorId);
    setComposerState(clearQueue(composerState));
  }, [composerState, withdrawQueuedPrompt]);

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
      persistDraft(value);
      if (composerRef.current) autoGrow(composerRef.current, shrank);
      setDraft(value);
    },
    [composerRef, persistDraft],
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
      if (event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        if (isBuiltInClearCommand(draftRef.current)) {
          sendDraft();
          return;
        }
      }
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
    draftRestoreGeneration,
  };

  return <ComposerContext.Provider value={value}>{children}</ComposerContext.Provider>;
}
