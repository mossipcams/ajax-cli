import {
  MAX_QUEUED_PROMPTS,
  OPEN_FAILURE,
  PROMPT_TOO_LONG,
  type PendingPrompt,
  type SocketListener,
  type WebSessionServerEvent,
  type WebSessionTransport,
  type WebSessionTransportCallbacks,
  type WebSessionTransportPlatform,
} from "./contracts";
import {
  createBrowserWebSessionPlatform,
  newPromptId,
  OPEN_READY_STATE,
  sessionSocketUrl,
  waitForSocketOpen,
} from "./commands";
import { clearSessionCursor, frameFits, readOutbox, writeOutbox } from "./outbox";
import { parseServerFrame } from "./parse";

export function connectWebSessionTransport(
  handle: string,
  callbacks: WebSessionTransportCallbacks,
  platform: WebSessionTransportPlatform = createBrowserWebSessionPlatform(),
  model?: string,
  resumeCursor?: number,
): WebSessionTransport {
  let socket: ReturnType<WebSessionTransportPlatform["openSocket"]> | undefined;
  let ready = false;
  let disposed = false;
  let replayEndCursor: number | undefined;
  let replaySettledEvent: WebSessionServerEvent | undefined;
  let inlineReplayCursors: Set<number> | undefined;
  let inlineTranscriptFloor: number | undefined;
  const stored = readOutbox(handle);
  const pendingPrompts = stored.filter(frameFits);
  if (pendingPrompts.length !== stored.length) writeOutbox(handle, pendingPrompts);
  clearSessionCursor(handle);

  const acknowledgePrompt = (event: WebSessionServerEvent) => {
    if (event.type !== "prompt_accepted") return;
    const index = pendingPrompts.findIndex(
      (prompt) => prompt.clientMessageId === event.clientMessageId,
    );
    if (index < 0) return;
    pendingPrompts.splice(index, 1);
    writeOutbox(handle, pendingPrompts);
  };

  const messageListener: SocketListener = (event) => {
    const messageEvent = event as MessageEvent;
    if (typeof messageEvent.data !== "string") return;
    const frame = parseServerFrame(messageEvent.data);
    if (!frame) return;

    if (frame.kind === "snapshot") {
      const inlineTranscript = frame.snapshot.transcript;
      const consumesInlineTranscript =
        inlineTranscript !== undefined && callbacks.onSnapshot !== undefined;
      if (consumesInlineTranscript) {
        // Tail snapshots only include the newest events: envelopes matching a
        // transcript cursor are exact dedupes, and replay below the floor is
        // stale. Socket-local errors (cursor 0) are exempt from the floor.
        inlineReplayCursors = new Set(inlineTranscript.map(({ cursor }) => cursor));
        inlineTranscriptFloor = frame.snapshot.transcriptFrom;
        callbacks.onCursorAdvance?.(frame.snapshot.cursor);
        for (const { event: transcriptEvent } of inlineTranscript) {
          acknowledgePrompt(transcriptEvent);
        }
      }
      callbacks.onSnapshot?.(frame.snapshot);
      if (frame.snapshot.agentPending === true) {
        // Early snapshot while the harness is still spawning/restoring: keep
        // inline-transcript bookkeeping, but do not mark ready, emit a
        // synthetic ready event, or flush queued prompts.
        callbacks.onAgentPending?.();
        return;
      }
      if (!ready) {
        ready = true;
        const readyEvent: WebSessionServerEvent = {
          type: "ready",
          model: frame.snapshot.model,
          busy: frame.snapshot.turnState === "busy",
          reset: frame.snapshot.reset,
        };
        if (!consumesInlineTranscript) {
          replayEndCursor = frame.snapshot.cursor;
          replaySettledEvent = { ...readyEvent, reset: false };
          callbacks.onEvent(readyEvent);
          if (frame.snapshot.pendingPermission) {
            const pending = frame.snapshot.pendingPermission;
            callbacks.onEvent({
              type: "permission_request",
              requestId: pending.requestId,
              ...(pending.title !== undefined ? { title: pending.title } : {}),
              ...(pending.detail !== undefined ? { detail: pending.detail } : {}),
            });
          }
          if (frame.snapshot.pendingElicitation) {
            const pending = frame.snapshot.pendingElicitation;
            callbacks.onEvent({
              type: "elicitation_request",
              requestId: pending.requestId,
              message: pending.message,
              schema: pending.schema,
            });
          }
        }
        callbacks.onReady(frame.snapshot.model.trim() || "auto");
        for (const prompt of pendingPrompts) sendPromptNow(prompt);
      } else {
        if (frame.snapshot.reset && !consumesInlineTranscript) {
          callbacks.onEvent({
            type: "ready",
            model: frame.snapshot.model,
            busy: frame.snapshot.turnState === "busy",
            reset: true,
          });
        }
        callbacks.onReady(frame.snapshot.model.trim() || "auto");
      }
      return;
    }

    const parsed = frame.event;
    if (inlineReplayCursors?.delete(frame.cursor)) {
      if (inlineReplayCursors.size === 0) inlineReplayCursors = undefined;
      return;
    }
    // Socket-local errors use cursor 0 but are not part of transcript replay.
    if (parsed.type === "error" && frame.cursor === 0) {
      callbacks.onEvent(parsed);
      return;
    }
    if (inlineTranscriptFloor !== undefined && frame.cursor < inlineTranscriptFloor) {
      acknowledgePrompt(parsed);
      return;
    }
    callbacks.onCursorAdvance?.(frame.cursor + 1);
    acknowledgePrompt(parsed);
    callbacks.onEvent(parsed);
    if (replayEndCursor !== undefined && frame.cursor + 1 === replayEndCursor) {
      const settled = replaySettledEvent;
      replayEndCursor = undefined;
      replaySettledEvent = undefined;
      if (settled) callbacks.onEvent(settled);
    } else if (replayEndCursor !== undefined && frame.cursor >= replayEndCursor) {
      replayEndCursor = undefined;
      replaySettledEvent = undefined;
    }
  };

  const closeListener: SocketListener = () => {
    if (!disposed) callbacks.onClosed();
  };

  function sendJson(payload: Record<string, unknown>) {
    if (!socket || socket.readyState !== OPEN_READY_STATE) return;
    socket.send(JSON.stringify(payload));
  }

  function sendPromptNow(prompt: PendingPrompt) {
    sendJson({
      type: "prompt",
      text: prompt.text,
      clientMessageId: prompt.clientMessageId,
      ...(prompt.contentBlocks?.length ? { contentBlocks: prompt.contentBlocks } : {}),
    });
  }

  socket = platform.openSocket(
    sessionSocketUrl(handle, model, resumeCursor, callbacks.onSnapshot !== undefined),
  );
  socket.addEventListener("message", messageListener);
  socket.addEventListener("close", closeListener);
  void waitForSocketOpen(socket).catch(() => {
    if (disposed) return;
    // Keep unaccepted prompts queued: clearing them here left the stored
    // outbox holding prompts the next write would silently overwrite.
    callbacks.onEvent({ type: "error", message: OPEN_FAILURE });
  });

  return {
    sendPrompt(text, contentBlocks = [], existingClientMessageId) {
      const trimmed = text.trim();
      if (!trimmed && !contentBlocks.length) return "";
      const prompt: PendingPrompt = {
        text: trimmed,
        clientMessageId: existingClientMessageId?.trim() || newPromptId(),
        ...(contentBlocks.length ? { contentBlocks } : {}),
      };
      if (!frameFits(prompt)) {
        callbacks.onEvent({ type: "error", message: PROMPT_TOO_LONG });
        return "";
      }
      if (pendingPrompts.length >= MAX_QUEUED_PROMPTS) {
        callbacks.onEvent({
          type: "error",
          message: "The prompt queue is full. Wait for a pending message to be accepted and try again.",
        });
        return "";
      }
      pendingPrompts.push(prompt);
      writeOutbox(handle, pendingPrompts);
      if (ready) sendPromptNow(prompt);
      return prompt.clientMessageId;
    },
    withdrawQueuedPrompt(clientMessageId) {
      if (!clientMessageId) return;
      const index = pendingPrompts.findIndex(
        (prompt) => prompt.clientMessageId === clientMessageId,
      );
      if (index >= 0) {
        pendingPrompts.splice(index, 1);
        writeOutbox(handle, pendingPrompts);
      }
      sendJson({ type: "prompt", text: "", clientMessageId, contentBlocks: [] });
    },
    sendCancel(keepQueue = false) {
      if (!keepQueue) {
        pendingPrompts.splice(0, pendingPrompts.length);
        writeOutbox(handle, pendingPrompts);
      }
      sendJson(keepQueue ? { type: "cancel", keepQueue: true } : { type: "cancel" });
    },
    sendClear() {
      pendingPrompts.splice(0, pendingPrompts.length);
      writeOutbox(handle, pendingPrompts);
      sendJson({ type: "clear" });
    },
    retryRestore() {
      sendJson({ type: "retry_restore" });
    },
    startFresh() {
      sendJson({ type: "start_fresh" });
    },
    setModel(nextModel) {
      const trimmed = nextModel.trim() || "auto";
      sendJson({ type: "set_config_option", configId: "model", value: trimmed });
    },
    setConfigOption(configId, value) {
      sendJson({
        type: "set_config_option",
        configId,
        value,
      });
    },
    respondPermission(requestId, approved, reason) {
      sendJson({
        type: "permission",
        requestId,
        approved,
        ...(reason ? { reason } : {}),
      });
    },
    respondElicitation(requestId, action, content) {
      sendJson({
        type: "elicitation",
        requestId,
        action,
        ...(action === "accept" && content ? { content } : {}),
      });
    },
    dispose() {
      disposed = true;
      socket?.removeEventListener("message", messageListener);
      socket?.removeEventListener("close", closeListener);
      try {
        socket?.close();
      } catch {
        // A failing close() during disposal is non-fatal; the socket will GC.
      }
      socket = undefined;
    },
  };
}
