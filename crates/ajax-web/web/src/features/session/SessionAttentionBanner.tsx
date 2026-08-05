import { useEffect, useState } from "react";
import type {
  SessionAttentionItem,
  SessionAttentionKind,
  SessionAttentionResponse,
} from "./types";

interface Props {
  currentHandle: string;
  items: SessionAttentionItem[];
  onRespond: (
    item: SessionAttentionItem,
    response: SessionAttentionResponse,
  ) => void;
  onOpenTask: (handle: string) => void;
}

function statusLabel(kind: SessionAttentionKind): string {
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

export default function SessionAttentionBanner({
  currentHandle,
  items,
  onRespond,
  onOpenTask,
}: Props) {
  const remote = items.filter((item) => item.handle !== currentHandle);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [replyDraft, setReplyDraft] = useState("");
  const [visible, setVisible] = useState(false);

  const primary = remote[0];
  const primaryKey = primary ? `${primary.handle}:${primary.requestId}` : "";

  useEffect(() => {
    if (!primaryKey) {
      setVisible(false);
      return;
    }
    setVisible(false);
    const id = window.requestAnimationFrame(() => setVisible(true));
    return () => window.cancelAnimationFrame(id);
  }, [primaryKey]);

  if (!primary) return null;

  const overflow = remote.length - 1;
  const isQuestionExpanded = expandedId === primaryKey && primary.kind === "question";

  const openTask = () => {
    if (primary.kind === "review") {
      onRespond(primary, { type: "review", action: "open" });
    }
    onOpenTask(primary.handle);
  };

  return (
    <div
      className={`ajax-web-session-attention-rail${visible ? " is-visible" : ""}`}
      data-testid="ajax-web-session-attention-rail"
      role="region"
      aria-label="Other sessions need attention"
    >
      <div
        className={`ajax-web-session-attention-banner is-${primary.kind}`}
        data-testid="ajax-web-session-attention-banner"
        data-kind={primary.kind}
        data-handle={primary.handle}
      >
        <div className="ajax-web-session-attention-status">
          {overflow > 0 ? (
            <p
              className="ajax-web-session-attention-overflow"
              data-testid="ajax-web-session-attention-overflow"
            >
              +{overflow} more
            </p>
          ) : null}
          <p className="ajax-web-session-attention-title">
            <span className="ajax-web-session-attention-kind">{statusLabel(primary.kind)}</span>
            <span className="ajax-web-session-attention-sep" aria-hidden="true">
              ·
            </span>
            <span className="ajax-web-session-attention-origin">{primary.handle}</span>
          </p>
          <p className="ajax-web-session-attention-summary">{primary.summary}</p>
        </div>

        <div className="ajax-web-session-attention-actions">
          {primary.kind === "permission" ? (
            <>
              <button
                type="button"
                className="ajax-web-session-attention-action is-primary"
                data-testid="ajax-web-session-attention-approve"
                onClick={() =>
                  onRespond(primary, { type: "permission", outcome: "allow-once" })
                }
              >
                Approve
              </button>
              <button
                type="button"
                className="ajax-web-session-attention-action"
                data-testid="ajax-web-session-attention-deny"
                onClick={() =>
                  onRespond(primary, { type: "permission", outcome: "reject" })
                }
              >
                Deny
              </button>
            </>
          ) : null}

          {primary.kind === "question" ? (
            <button
              type="button"
              className="ajax-web-session-attention-action is-primary"
              data-testid="ajax-web-session-attention-reply"
              onClick={() => {
                setExpandedId(isQuestionExpanded ? null : primaryKey);
                setReplyDraft("");
              }}
            >
              {isQuestionExpanded ? "Cancel" : "Reply"}
            </button>
          ) : null}

          {primary.kind === "failed" ? (
            <>
              <button
                type="button"
                className="ajax-web-session-attention-action is-primary"
                data-testid="ajax-web-session-attention-retry"
                onClick={() => onRespond(primary, { type: "failed", action: "retry" })}
              >
                Retry
              </button>
              <button
                type="button"
                className="ajax-web-session-attention-action"
                data-testid="ajax-web-session-attention-stop"
                onClick={() => onRespond(primary, { type: "failed", action: "stop" })}
              >
                Stop
              </button>
            </>
          ) : null}

          <button
            type="button"
            className={`ajax-web-session-attention-action${primary.kind === "review" ? " is-primary" : ""}`}
            data-testid="ajax-web-session-attention-open"
            onClick={openTask}
          >
            Open
          </button>
        </div>

        {isQuestionExpanded ? (
          <form
            className="ajax-web-session-attention-composer"
            data-testid="ajax-web-session-attention-composer"
            onSubmit={(event) => {
              event.preventDefault();
              const text = replyDraft.trim();
              if (!text) return;
              onRespond(primary, { type: "question", text });
              setReplyDraft("");
              setExpandedId(null);
            }}
          >
            <textarea
              className="ajax-web-session-attention-input"
              data-testid="ajax-web-session-attention-input"
              value={replyDraft}
              rows={2}
              placeholder="Answer the agent…"
              onChange={(event) => setReplyDraft(event.target.value)}
            />
            <button
              type="submit"
              className="ajax-web-session-attention-action is-primary"
              data-testid="ajax-web-session-attention-send-reply"
              disabled={replyDraft.trim().length === 0}
            >
              Send
            </button>
          </form>
        ) : null}
      </div>
    </div>
  );
}
