import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  connectWebSession,
  composeWebSessionPrompt,
  type WebSessionTransport,
} from "./webSessionTransport";
import SessionAttentionBanner from "./SessionAttentionBanner";
import SessionComposerKeys from "./SessionComposerKeys";
import SymbolDetailSheet from "./SymbolDetailSheet";
import SymbolSearchSheet from "./SymbolSearchSheet";
import {
  buildKnownSymbolIndex,
  mergeKnownSymbols,
  renderMessageContent,
} from "./renderMessage";
import { deleteBackward, insertAtSelection, type DraftSelection } from "./sessionDraftEdit";
import { useSessionComposerSpeech } from "./useSessionComposerSpeech";
import {
  symbolContextChipLabel,
  type SessionAttentionItem,
  type SessionAttentionResponse,
  type WebSessionConnectionStatus,
  type WebSessionMessage,
  type WebSessionRunStatus,
  type WebSessionSymbolContext,
} from "./types";
import type { BrowserTaskCard } from "@/shared/lib/types";

interface Props {
  handle: string;
  cockpitCards?: BrowserTaskCard[];
  onOpenTask?: (handle: string) => void;
}

let nextMessageId = 0;

function newMessageId(): string {
  nextMessageId += 1;
  return `web-session-msg-${nextMessageId}`;
}

function statusChipTone(
  connectionStatus: WebSessionConnectionStatus,
  runStatus: WebSessionRunStatus | null,
  errorMessage: string | null,
): { tone: string; label: string } {
  if (connectionStatus === "error" || errorMessage) {
    return { tone: "error", label: "Error" };
  }
  if (connectionStatus === "reconnecting") {
    return { tone: "waiting", label: "Reconnecting" };
  }
  if (runStatus === "running") {
    return { tone: "running", label: "Running" };
  }
  if (runStatus === "waiting") {
    return { tone: "waiting", label: "Waiting" };
  }
  if (connectionStatus === "connecting") {
    return { tone: "waiting", label: "Connecting" };
  }
  return { tone: "waiting", label: "Ready" };
}

function attentionKey(item: SessionAttentionItem): string {
  return `${item.handle}::${item.requestId}`;
}

function cockpitDerivedAttentions(
  currentHandle: string,
  cards: BrowserTaskCard[],
): SessionAttentionItem[] {
  const items: SessionAttentionItem[] = [];
  for (const card of cards) {
    if (card.qualified_handle === currentHandle) continue;
    if (card.attention === "review") {
      items.push({
        handle: card.qualified_handle,
        requestId: `review:${card.qualified_handle}`,
        kind: "review",
        title: "Ready for review",
        summary: card.status_explanation?.trim() || card.title || "Open for review",
      });
    }
  }
  return items;
}

export default function AjaxWebSessionView({ handle, cockpitCards = [], onOpenTask }: Props) {
  const [messages, setMessages] = useState<WebSessionMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [attachedSymbols, setAttachedSymbols] = useState<WebSessionSymbolContext[]>([]);
  const [knownSymbols, setKnownSymbols] = useState<WebSessionSymbolContext[]>([]);
  const [symbolSheetOpen, setSymbolSheetOpen] = useState(false);
  const [detailSymbol, setDetailSymbol] = useState<WebSessionSymbolContext | null>(null);
  const [connectionStatus, setConnectionStatus] =
    useState<WebSessionConnectionStatus>("connecting");
  const [runStatus, setRunStatus] = useState<WebSessionRunStatus | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [attentions, setAttentions] = useState<SessionAttentionItem[]>([]);
  const transportRef = useRef<WebSessionTransport | null>(null);
  const messagesEndRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLTextAreaElement | null>(null);
  const assistantStreamingRef = useRef(false);

  const appendAssistantDelta = useCallback((delta: string) => {
    if (!delta) return;
    setMessages((prev) => {
      const last = prev.at(-1);
      if (last?.role === "assistant" && last.streaming) {
        return prev.slice(0, -1).concat({ ...last, text: last.text + delta });
      }
      assistantStreamingRef.current = true;
      return prev.concat({
        id: newMessageId(),
        role: "assistant",
        text: delta,
        streaming: true,
      });
    });
  }, []);

  const settleAssistant = useCallback(() => {
    assistantStreamingRef.current = false;
    setMessages((prev) => {
      const last = prev.at(-1);
      if (!last || last.role !== "assistant" || !last.streaming) return prev;
      return prev.slice(0, -1).concat({ ...last, streaming: false });
    });
  }, []);

  const upsertAttention = useCallback((item: SessionAttentionItem) => {
    setAttentions((prev) => {
      const key = attentionKey(item);
      const without = prev.filter((existing) => attentionKey(existing) !== key);
      return without.concat(item);
    });
  }, []);

  const clearAttention = useCallback((targetHandle: string, requestId: string) => {
    setAttentions((prev) =>
      prev.filter((item) => !(item.handle === targetHandle && item.requestId === requestId)),
    );
  }, []);

  useEffect(() => {
    const transport = connectWebSession(handle, {
      onConnectionStatus: setConnectionStatus,
      onSessionReady: () => {
        setErrorMessage(null);
      },
      onRunStatus: setRunStatus,
      onAssistantDelta: appendAssistantDelta,
      onSettled: () => {
        settleAssistant();
        setRunStatus("waiting");
      },
      onError: (message) => {
        setErrorMessage(message);
        settleAssistant();
      },
      onClosed: () => {
        settleAssistant();
      },
      onAttentionRequired: upsertAttention,
      onAttentionCleared: clearAttention,
      onAttentionError: (_targetHandle, _requestId, message) => {
        setErrorMessage(message);
      },
    });
    transportRef.current = transport;
    return () => {
      transport.dispose();
      transportRef.current = null;
    };
  }, [handle, appendAssistantDelta, settleAssistant, upsertAttention, clearAttention]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView?.({ block: "end" });
  }, [messages]);

  const applyDraftSelection = useCallback((next: DraftSelection) => {
    setDraft(next.value);
  }, []);

  const appendDraftText = useCallback((text: string) => {
    setDraft((prev) => {
      const input = inputRef.current;
      const start = input?.selectionStart ?? prev.length;
      const end = input?.selectionEnd ?? prev.length;
      const next = insertAtSelection(
        { value: prev, selectionStart: start, selectionEnd: end },
        text,
      );
      requestAnimationFrame(() => {
        if (inputRef.current) {
          inputRef.current.focus();
          inputRef.current.setSelectionRange(next.selectionStart, next.selectionEnd);
        }
      });
      return next.value;
    });
  }, []);

  const deleteDraftBackward = useCallback((charCount: number) => {
    setDraft((prev) => {
      let state: DraftSelection = {
        value: prev,
        selectionStart: prev.length,
        selectionEnd: prev.length,
      };
      for (let i = 0; i < charCount; i += 1) {
        state = deleteBackward(state);
      }
      return state.value;
    });
  }, []);

  const {
    speechModel,
    micAriaLabel,
    micArmed,
    toggleMic,
    cancelSpeechInput,
  } = useSessionComposerSpeech({
    handle,
    appendDraftText,
    deleteDraftBackward,
  });

  const knownSymbolIndex = useMemo(() => buildKnownSymbolIndex(knownSymbols), [knownSymbols]);

  const rememberSymbols = useCallback((symbols: Iterable<WebSessionSymbolContext>) => {
    setKnownSymbols((prev) => mergeKnownSymbols(prev, symbols));
  }, []);

  const mergedAttentions = useMemo(() => {
    const derived = cockpitDerivedAttentions(handle, cockpitCards);
    const byKey = new Map<string, SessionAttentionItem>();
    for (const item of attentions) {
      byKey.set(attentionKey(item), item);
    }
    for (const item of derived) {
      const key = attentionKey(item);
      if (!byKey.has(key)) byKey.set(key, item);
    }
    return Array.from(byKey.values());
  }, [attentions, cockpitCards, handle]);

  const canSend =
    connectionStatus === "connected" && runStatus !== "running" && draft.trim().length > 0;

  const sendPrompt = () => {
    const text = draft.trim();
    if (!text || !transportRef.current) return;
    const prompt = composeWebSessionPrompt(text, attachedSymbols);
    rememberSymbols(attachedSymbols);
    setMessages((prev) =>
      prev.concat({
        id: newMessageId(),
        role: "user",
        text,
      }),
    );
    setDraft("");
    setAttachedSymbols([]);
    setErrorMessage(null);
    setRunStatus("running");
    transportRef.current.sendPrompt(prompt);
  };

  const abort = () => {
    transportRef.current?.sendAbort();
  };

  const retryConnection = () => {
    setErrorMessage(null);
    transportRef.current?.reconnectNow();
  };

  const removeSymbol = (symbolId: string) => {
    setAttachedSymbols((prev) => prev.filter((symbol) => symbol.id !== symbolId));
  };

  const attachSymbol = (symbol: WebSessionSymbolContext) => {
    rememberSymbols([symbol]);
    setAttachedSymbols((prev) => {
      if (prev.some((item) => item.id === symbol.id)) return prev;
      return prev.concat(symbol);
    });
    setDetailSymbol(null);
  };

  const respondAttention = (item: SessionAttentionItem, response: SessionAttentionResponse) => {
    transportRef.current?.respondAttention(item.handle, item.requestId, response);
    if (response.type === "review") {
      clearAttention(item.handle, item.requestId);
    }
  };

  const dismissSheets = () => {
    setSymbolSheetOpen(false);
    setDetailSymbol(null);
    cancelSpeechInput();
  };

  const { tone, label } = statusChipTone(connectionStatus, runStatus, errorMessage);
  const showStop = connectionStatus === "connected" && runStatus === "running";
  const showRetry = connectionStatus === "error";
  const composerEnabled = connectionStatus === "connected";

  return (
    <section
      className="ajax-web-session"
      data-testid="ajax-web-session"
      data-handle={handle}
      aria-labelledby="ajax-web-session-heading"
    >
      <header className="ajax-web-session-header">
        <h2 id="ajax-web-session-heading" className="ajax-web-session-title">
          Ajax Web Session
        </h2>
        <span
          className={`ajax-web-session-status interact-pill tone-${tone}`}
          data-testid="ajax-web-session-status"
        >
          {label}
        </span>
      </header>

      <SessionAttentionBanner
        currentHandle={handle}
        items={mergedAttentions}
        onRespond={respondAttention}
        onOpenTask={(target) => onOpenTask?.(target)}
      />

      {errorMessage || showRetry ? (
        <div className="ajax-web-session-error-row" role="alert" data-testid="ajax-web-session-error">
          <p className="ajax-web-session-error">
            {errorMessage ?? "Web session connection failed. Check the host and tap Retry."}
          </p>
          {showRetry ? (
            <button
              type="button"
              className="ajax-web-session-action is-retry"
              data-testid="ajax-web-session-retry"
              onClick={retryConnection}
            >
              Retry
            </button>
          ) : null}
        </div>
      ) : null}

      <div className="ajax-web-session-messages" data-testid="ajax-web-session-messages">
        {messages.length === 0 ? (
          <p className="ajax-web-session-empty" data-testid="ajax-web-session-empty">
            {connectionStatus === "connecting" || connectionStatus === "reconnecting"
              ? "Connecting to Cursor on the host…"
              : "Message Cursor. Attach symbols for context. Other sessions that need you appear above."}
          </p>
        ) : null}
        {messages.map((message) => (
          <div
            key={message.id}
            className={`ajax-web-session-bubble is-${message.role}${message.streaming ? " is-streaming" : ""}`}
            data-testid={`ajax-web-session-bubble-${message.role}`}
          >
            {renderMessageContent(message.text, knownSymbolIndex, setDetailSymbol)}
          </div>
        ))}
        <div ref={messagesEndRef} />
      </div>

      <footer className="ajax-web-session-composer" data-testid="ajax-web-session-composer">
        {attachedSymbols.length > 0 ? (
          <div className="ajax-web-session-context-chips" data-testid="ajax-web-session-context-chips">
            {attachedSymbols.map((symbol) => (
              <button
                key={symbol.id}
                type="button"
                className="ajax-web-session-context-chip"
                data-testid={`ajax-web-session-context-chip-${symbol.id}`}
                onClick={() => removeSymbol(symbol.id)}
                aria-label={`Remove ${symbol.name}`}
              >
                <span>{symbolContextChipLabel(symbol)}</span>
                <span aria-hidden="true">×</span>
              </button>
            ))}
          </div>
        ) : null}

        <div className="ajax-web-session-composer-row">
          <button
            type="button"
            className="ajax-web-session-add-context"
            data-testid="ajax-web-session-add-context"
            disabled={!composerEnabled}
            onClick={() => setSymbolSheetOpen(true)}
          >
            Add context
          </button>
          <textarea
            ref={inputRef}
            className="ajax-web-session-input"
            data-testid="ajax-web-session-input"
            value={draft}
            placeholder="Message the agent…"
            rows={3}
            disabled={!composerEnabled}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                if (canSend) sendPrompt();
              }
            }}
          />
        </div>

        <div className="ajax-web-session-composer-actions">
          {showStop ? (
            <button
              type="button"
              className="ajax-web-session-action is-stop"
              data-testid="ajax-web-session-stop"
              onClick={abort}
            >
              Stop
            </button>
          ) : (
            <button
              type="button"
              className="ajax-web-session-action is-send"
              data-testid="ajax-web-session-send"
              disabled={!canSend}
              onClick={sendPrompt}
            >
              Send
            </button>
          )}
        </div>

        <SessionComposerKeys
          inputRef={inputRef}
          draft={draft}
          onDraftChange={applyDraftSelection}
          runStatus={runStatus}
          onAbort={abort}
          onDismissSheets={dismissSheets}
          micArmed={micArmed}
          micAriaLabel={micAriaLabel}
          micDisabled={
            speechModel.state === "connecting" || speechModel.state === "finalizing"
          }
          onToggleMic={toggleMic}
        />
      </footer>

      <SymbolSearchSheet
        handle={handle}
        open={symbolSheetOpen}
        selected={attachedSymbols}
        onClose={() => setSymbolSheetOpen(false)}
        onConfirm={(symbols) => {
          rememberSymbols(symbols);
          setAttachedSymbols(symbols);
          setSymbolSheetOpen(false);
        }}
      />

      <SymbolDetailSheet
        symbol={detailSymbol}
        open={detailSymbol !== null}
        attached={attachedSymbols}
        onClose={() => setDetailSymbol(null)}
        onAttach={attachSymbol}
      />
    </section>
  );
}
