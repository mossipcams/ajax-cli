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
  const stored = readOutbox(handle);
  const pendingPrompts = stored.filter(frameFits);
  if (pendingPrompts.length !== stored.length) writeOutbox(handle, pendingPrompts);
  // ponytail: cursor is in-memory only; drop any legacy sessionStorage value on attach.
  clearSessionCursor(handle);

  const messageListener: SocketListener = (event) => {
    const messageEvent = event as MessageEvent;
    if (typeof messageEvent.data !== "string") return;
    const frame = parseServerFrame(messageEvent.data);
    if (!frame) return;

    if (frame.kind === "snapshot") {
      callbacks.onSnapshot?.(frame.snapshot);
      if (!ready) {
        ready = true;
        replayEndCursor = frame.snapshot.cursor;
        const readyEvent: WebSessionServerEvent = {
          type: "ready",
          model: frame.snapshot.model,
          busy: frame.snapshot.turnState === "busy",
          reset: frame.snapshot.reset,
        };
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
        callbacks.onReady(frame.snapshot.model.trim() || "auto");
        for (const prompt of pendingPrompts) sendPromptNow(prompt);
      } else {
        if (frame.snapshot.reset) {
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

    callbacks.onCursorAdvance?.(frame.cursor + 1);
    const parsed = frame.event;
    if (parsed.type === "prompt_accepted") {
      const index = pendingPrompts.findIndex(
        (prompt) => prompt.clientMessageId === parsed.clientMessageId,
      );
      if (index >= 0) {
        pendingPrompts.splice(index, 1);
        writeOutbox(handle, pendingPrompts);
      }
    }
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

  socket = platform.openSocket(sessionSocketUrl(handle, model, resumeCursor));
  socket.addEventListener("message", messageListener);
  socket.addEventListener("close", closeListener);
  void waitForSocketOpen(socket).catch(() => {
    if (disposed) return;
    pendingPrompts.length = 0;
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
        pendingPrompts.shift();
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
        // ignore close races
      }
      socket = undefined;
    },
  };
}
