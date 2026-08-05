import { useState } from "react";
import type { SessionAttentionItem, SessionAttentionResponse } from "./types";

interface Props {
  currentHandle: string;
  items: SessionAttentionItem[];
  onRespond: (
    item: SessionAttentionItem,
    response: SessionAttentionResponse,
  ) => void;
  onOpenTask: (handle: string) => void;
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

  if (remote.length === 0) return null;

  const primary = remote[0];
  const overflow = remote.length - 1;
  const expandedKey = `${primary.handle}:${primary.requestId}`;
  const isQuestionExpanded = expandedId === expandedKey && primary.kind === "question";

  return (
    <div
      className="ajax-web-session-attention-rail"
      data-testid="ajax-web-session-attention-rail"
      role="region"
      aria-label="Other sessions need attention"
    >
      {overflow > 0 ? (
        <p className="ajax-web-session-attention-overflow" data-testid="ajax-web-session-attention-overflow">
          {overflow + 1} sessions need you
        </p>
      ) : null}

      <article
        className={`ajax-web-session-attention-banner is-${primary.kind}`}
        data-testid="ajax-web-session-attention-banner"
        data-kind={primary.kind}
        data-handle={primary.handle}
      >
        <div className="ajax-web-session-attention-copy">
          <p className="ajax-web-session-attention-origin">{primary.handle}</p>
          <p className="ajax-web-session-attention-title">{primary.title}</p>
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
                onClick={() => onRespond(primary, { type: "permission", outcome: "reject" })}
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
                setExpandedId(isQuestionExpanded ? null : expandedKey);
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
                className="ajax-web-session-attention-action"
                data-testid="ajax-web-session-attention-stop"
                onClick={() => onRespond(primary, { type: "failed", action: "stop" })}
              >
                Stop
              </button>
              <button
                type="button"
                className="ajax-web-session-attention-action is-primary"
                data-testid="ajax-web-session-attention-retry"
                onClick={() => onRespond(primary, { type: "failed", action: "retry" })}
              >
                Retry
              </button>
            </>
          ) : null}

          {primary.kind === "review" ? (
            <button
              type="button"
              className="ajax-web-session-attention-action is-primary"
              data-testid="ajax-web-session-attention-open"
              onClick={() => {
                onRespond(primary, { type: "review", action: "open" });
                onOpenTask(primary.handle);
              }}
            >
              Open
            </button>
          ) : null}
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
      </article>
    </div>
  );
}
