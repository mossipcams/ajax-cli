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
import {
  buildSessionFeed,
  hasCurrentHandleDecisionPending,
  truncateProgressText,
} from "./sessionCards";
import { useSessionComposerSpeech } from "./useSessionComposerSpeech";
import {
  symbolContextChipLabel,
  type SessionAttentionItem,
  type SessionAttentionResponse,
  type SessionComposerMode,
  type SessionDecisionCard,
  type WebSessionConnectionStatus,
  type WebSessionMessage,
  type WebSessionProgress,
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

function decisionStatusLabel(kind: SessionAttentionItem["kind"]): string {
  switch (kind) {
    case "permission":
      return "Needs permission";
    case "question":
      return "Needs answer";
    case "failed":
      return "Run failed";
    case "review":
      return "Ready for review";
  }
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
  const [progress, setProgress] = useState<WebSessionProgress[]>([]);
  const [draft, setDraft] = useState("");
  const [composerMode, setComposerMode] = useState<SessionComposerMode>("hidden");
  const [questionAttention, setQuestionAttention] = useState<SessionAttentionItem | null>(null);
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
  const feedEndRef = useRef<HTMLDivElement | null>(null);
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

  const appendProgress = useCallback((item: Omit<WebSessionProgress, "id">) => {
    setProgress((prev) => prev.concat({ ...item, id: newMessageId() }));
  }, []);

  useEffect(() => {
    const transport = connectWebSession(handle, {
      onConnectionStatus: setConnectionStatus,
      onSessionReady: () => {
        setErrorMessage(null);
      },
      onRunStatus: setRunStatus,
      onAssistantDelta: appendAssistantDelta,
      onProgress: appendProgress,
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
  }, [handle, appendAssistantDelta, appendProgress, settleAssistant, upsertAttention, clearAttention]);

  useEffect(() => {
    if (runStatus === "running") {
      setComposerMode("hidden");
      setQuestionAttention(null);
    }
  }, [runStatus]);

  useEffect(() => {
    feedEndRef.current?.scrollIntoView?.({ block: "end" });
  }, [messages, attentions]);

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

  const feedCards = useMemo(
    () => buildSessionFeed(messages, handle, mergedAttentions, progress),
    [messages, handle, mergedAttentions, progress],
  );

  const composerVisible = composerMode !== "hidden" && runStatus !== "running";
  const composerEnabled = connectionStatus === "connected" && composerVisible;
  const canSend = composerEnabled && draft.trim().length > 0;
  const decisionPending = hasCurrentHandleDecisionPending(handle, mergedAttentions);
  const showStop = connectionStatus === "connected" && runStatus === "running";
  const showRetry = connectionStatus === "error";
  const showContinue =
    connectionStatus === "connected" &&
    runStatus === "waiting" &&
    composerMode === "hidden" &&
    !decisionPending &&
    messages.length > 0;
  const showRedirect =
    connectionStatus === "connected" && runStatus !== "running" && composerMode === "hidden";

  const hideComposer = () => {
    setComposerMode("hidden");
    setQuestionAttention(null);
    setDraft("");
    setAttachedSymbols([]);
    cancelSpeechInput();
  };

  const openRedirectComposer = () => {
    setQuestionAttention(null);
    setComposerMode("redirect");
  };

  const openQuestionComposer = (item: SessionAttentionItem) => {
    setQuestionAttention(item);
    setComposerMode("question");
    setDraft("");
  };

  const sendPrompt = (text: string, symbols: WebSessionSymbolContext[] = []) => {
    if (!text || !transportRef.current) return;
    const prompt = composeWebSessionPrompt(text, symbols);
    rememberSymbols(symbols);
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

  const handleComposerSend = () => {
    const text = draft.trim();
    if (!text) return;
    if (composerMode === "question" && questionAttention) {
      respondAttention(questionAttention, { type: "question", text });
      hideComposer();
      return;
    }
    sendPrompt(text, attachedSymbols);
    hideComposer();
  };

  const sendContinue = () => {
    sendPrompt("Continue");
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

  const openReviewTask = (item: SessionDecisionCard["attention"]) => {
    respondAttention(item, { type: "review", action: "open" });
    onOpenTask?.(item.handle);
  };

  const dismissSheets = () => {
    setSymbolSheetOpen(false);
    setDetailSymbol(null);
    cancelSpeechInput();
  };

  const { tone, label } = statusChipTone(connectionStatus, runStatus, errorMessage);

  const renderDecisionCard = (card: SessionDecisionCard) => {
    const { attention } = card;
    return (
      <article
        key={card.id}
        className={`ajax-web-session-card ajax-web-session-card-decision is-${attention.kind}`}
        data-testid="ajax-web-session-card-decision"
        data-kind={attention.kind}
      >
        <p className="ajax-web-session-card-label">{decisionStatusLabel(attention.kind)}</p>
        <p className="ajax-web-session-card-body">{attention.summary}</p>
        <div className="ajax-web-session-card-actions">
          {attention.kind === "permission" ? (
            <>
              <button
                type="button"
                className="ajax-web-session-card-action is-primary"
                data-testid="ajax-web-session-decision-approve"
                onClick={() =>
                  respondAttention(attention, { type: "permission", outcome: "allow-once" })
                }
              >
                Approve
              </button>
              <button
                type="button"
                className="ajax-web-session-card-action"
                data-testid="ajax-web-session-decision-deny"
                onClick={() =>
                  respondAttention(attention, { type: "permission", outcome: "reject" })
                }
              >
                Deny
              </button>
            </>
          ) : null}
          {attention.kind === "question" ? (
            <>
              {attention.options?.map((option) => (
                <button
                  key={option}
                  type="button"
                  className="ajax-web-session-card-action is-primary"
                  data-testid={`ajax-web-session-decision-option-${option}`}
                  onClick={() => respondAttention(attention, { type: "question", text: option })}
                >
                  {option}
                </button>
              ))}
              <button
                type="button"
                className="ajax-web-session-card-action is-primary"
                data-testid="ajax-web-session-decision-reply"
                onClick={() => openQuestionComposer(attention)}
              >
                Reply
              </button>
            </>
          ) : null}
          {attention.kind === "failed" ? (
            <>
              <button
                type="button"
                className="ajax-web-session-card-action is-primary"
                data-testid="ajax-web-session-decision-retry"
                onClick={() => respondAttention(attention, { type: "failed", action: "retry" })}
              >
                Retry
              </button>
              <button
                type="button"
                className="ajax-web-session-card-action"
                data-testid="ajax-web-session-decision-stop"
                onClick={() => respondAttention(attention, { type: "failed", action: "stop" })}
              >
                Stop
              </button>
            </>
          ) : null}
          {attention.kind === "review" ? (
            <button
              type="button"
              className="ajax-web-session-card-action is-primary"
              data-testid="ajax-web-session-decision-review"
              onClick={() => openReviewTask(attention)}
            >
              Review
            </button>
          ) : null}
        </div>
      </article>
    );
  };

  return (
    <section
      className="ajax-web-session"
      data-testid="ajax-web-session"
      data-handle={handle}
      aria-labelledby="ajax-web-session-heading"
    >
      <header className="ajax-web-session-header">
        <h2 id="ajax-web-session-heading" className="ajax-web-session-title">
          Session
        </h2>
        <div className="ajax-web-session-header-actions">
          {showStop ? (
            <button
              type="button"
              className="ajax-web-session-action is-stop"
              data-testid="ajax-web-session-stop"
              onClick={abort}
            >
              Stop
            </button>
          ) : null}
          <span
            className={`ajax-web-session-status interact-pill tone-${tone}`}
            data-testid="ajax-web-session-status"
          >
            {label}
          </span>
        </div>
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

      <div className="ajax-web-session-feed" data-testid="ajax-web-session-feed">
        {feedCards.length === 0 ? (
          <div className="ajax-web-session-empty" data-testid="ajax-web-session-empty">
            <p className="ajax-web-session-empty-lead">
              {connectionStatus === "connecting" || connectionStatus === "reconnecting"
                ? "Connecting to Cursor on the host…"
                : "Supervise this run. Progress and decisions appear here."}
            </p>
            {connectionStatus === "connected" || connectionStatus === "connecting" ? (
              <p className="ajax-web-session-empty-hint">
                Cross-session needs appear above. Redirect when you need to steer.
              </p>
            ) : null}
          </div>
        ) : null}
        {feedCards.map((card) => {
          if (card.kind === "operator") {
            return (
              <article
                key={card.id}
                className="ajax-web-session-card ajax-web-session-card-operator"
                data-testid="ajax-web-session-card-operator"
              >
                <p className="ajax-web-session-card-label">Operator</p>
                <p className="ajax-web-session-card-body">{card.text}</p>
              </article>
            );
          }
          if (card.kind === "decision") {
            return renderDecisionCard(card);
          }
          if (card.kind === "tool" || card.kind === "file") {
            return (
              <article
                key={card.id}
                className={`ajax-web-session-card ajax-web-session-card-progress is-${card.kind}`}
                data-testid={`ajax-web-session-card-${card.kind}`}
              >
                <p className="ajax-web-session-card-label">
                  {card.kind === "tool" ? card.toolName || "Tool" : "File"}
                  <span className="ajax-web-session-card-live">{card.status}</span>
                </p>
                <p className="ajax-web-session-card-body">
                  {card.path ? `${card.path}: ` : ""}
                  {truncateProgressText(card.summary)}
                </p>
              </article>
            );
          }
          return (
            <article
              key={card.id}
              className={`ajax-web-session-card ajax-web-session-card-progress${card.streaming ? " is-streaming" : ""}`}
              data-testid="ajax-web-session-card-progress"
            >
              <p className="ajax-web-session-card-label">
                Progress
                {card.streaming ? (
                  <span className="ajax-web-session-card-live">Streaming</span>
                ) : null}
              </p>
              <div className="ajax-web-session-card-body">
                {renderMessageContent(
                  truncateProgressText(card.text),
                  knownSymbolIndex,
                  setDetailSymbol,
                )}
              </div>
            </article>
          );
        })}
        <div ref={feedEndRef} />
      </div>

      {showContinue || showRedirect ? (
        <div className="ajax-web-session-controls" data-testid="ajax-web-session-controls">
          {showContinue ? (
            <button
              type="button"
              className="ajax-web-session-control is-continue"
              data-testid="ajax-web-session-continue"
              onClick={sendContinue}
            >
              Continue
            </button>
          ) : null}
          {showRedirect ? (
            <button
              type="button"
              className="ajax-web-session-control is-redirect"
              data-testid="ajax-web-session-redirect"
              onClick={openRedirectComposer}
            >
              Redirect
            </button>
          ) : null}
        </div>
      ) : null}

      {composerVisible ? (
        <footer className="ajax-web-session-composer" data-testid="ajax-web-session-composer">
          {attachedSymbols.length > 0 ? (
            <div
              className="ajax-web-session-context-chips"
              data-testid="ajax-web-session-context-chips"
            >
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
              aria-label="Add context"
            >
              +
            </button>
            <textarea
              ref={inputRef}
              className="ajax-web-session-input"
              data-testid="ajax-web-session-input"
              value={draft}
              placeholder={
                composerMode === "question" ? "Answer the agent…" : "Redirect Cursor…"
              }
              rows={1}
              disabled={!composerEnabled}
              onChange={(event) => setDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey) {
                  event.preventDefault();
                  if (canSend) handleComposerSend();
                }
              }}
            />
            <button
              type="button"
              className="ajax-web-session-action is-send"
              data-testid="ajax-web-session-send"
              disabled={!canSend}
              onClick={handleComposerSend}
            >
              Send
            </button>
            <button
              type="button"
              className="ajax-web-session-action is-dismiss"
              data-testid="ajax-web-session-dismiss"
              onClick={hideComposer}
            >
              Cancel
            </button>
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
      ) : null}

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
