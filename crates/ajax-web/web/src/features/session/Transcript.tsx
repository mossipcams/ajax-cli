// The conversation: everything ACP sent, in the order it sent it. Prose,
// reasoning, tool calls with their output, the plan, and a marker where the
// agent asked for permission.
//
// Only the last agent row streams while busy; earlier rows stay settled so this
// list can hold scroll position.

import { memo, useState } from "react";
import Markdown from "./Markdown";
import ToolCard from "./ToolCard";
import type { ConversationItem, PlanEntry } from "./sessionThread";

/** Reasoning is the agent's account of its own turn: worth keeping, never worth
 * outranking the answer. Collapsed to one line until asked for, except while it
 * is the live tail of a busy turn. */
function Thinking({ text, live }: { text: string; live: boolean }) {
  const [manualOpen, setManualOpen] = useState(false);
  const expanded = live || manualOpen;
  return (
    <div className="session-thinking" data-testid="session-thinking">
      <button
        type="button"
        className="session-thinking-toggle"
        aria-expanded={expanded}
        onClick={() => setManualOpen(!manualOpen)}
      >
        <span className="session-thinking-mark" aria-hidden="true">
          ∴
        </span>
        Thinking
      </button>
      {expanded ? (
        <p className="session-thinking-body" data-testid="session-thinking-body">
          {text}
        </p>
      ) : null}
    </div>
  );
}

const PLAN_MARKS: Record<string, string> = {
  completed: "✓",
  in_progress: "▸",
  pending: "·",
};

function PlanChecklist({ entries }: { entries: PlanEntry[] }) {
  return (
    <ol className="session-plan" data-testid="session-plan">
      {entries.map((entry, index) => (
        <li
          key={`${index}-${entry.content}`}
          className="session-plan-step"
          data-status={entry.status}
        >
          <span className="session-plan-mark" aria-hidden="true">
            {PLAN_MARKS[entry.status] ?? "·"}
          </span>
          <span className="session-plan-text">{entry.content}</span>
        </li>
      ))}
    </ol>
  );
}

const Row = memo(function Row({ item, live }: { item: ConversationItem; live: boolean }) {
  switch (item.kind) {
    case "prose":
      return item.role === "user" ? (
        <article className="session-said" data-testid="session-message-user">
          {item.text}
        </article>
      ) : (
        <article
          className={live ? "session-reply is-live" : "session-reply"}
          data-testid="session-message-agent"
          {...(live ? { "data-live": "true" } : {})}
        >
          <Markdown source={item.text} live={live} />
        </article>
      );

    case "thought":
      return <Thinking text={item.text} live={live} />;

    case "tool":
      return <ToolCard call={item.call} />;

    case "plan":
      return <PlanChecklist entries={item.entries} />;

    // The buttons are in the head, which cannot scroll away. This row is the
    // history: it says the agent asked here, and how it ended.
    case "permission":
      return (
        <div
          className={`session-note tone-${item.resolved ? "muted" : "waiting"}`}
          data-testid="session-permission-marker"
          data-resolved={item.resolved ? "true" : "false"}
        >
          <span className="session-note-text">
            {item.resolved ? `Answered · ${item.title}` : `Permission requested · ${item.title}`}
          </span>
        </div>
      );

    case "note":
      return (
        <div
          className={`session-note tone-${item.tone === "error" ? "error" : "muted"}`}
          data-testid={`session-note-${item.tone}`}
        >
          <span className="session-note-text">{item.text}</span>
        </div>
      );
  }
});

/** Only the tail item changes while a turn streams; every settled row above it
 * is referentially stable, so `memo` keeps a long conversation off the hot path. */
export default function Transcript({
  items,
  busy,
}: {
  items: ConversationItem[];
  busy: boolean;
}) {
  const lastAgentProseId = (() => {
    for (let i = items.length - 1; i >= 0; i -= 1) {
      const item = items[i];
      if (item.kind === "prose" && item.role === "agent") return item.id;
    }
    return null;
  })();
  const liveThoughtId =
    busy && items[items.length - 1]?.kind === "thought" ? items[items.length - 1].id : null;

  return (
    <>
      {items.map((item) => (
        <Row
          key={item.id}
          item={item}
          live={
            item.kind === "thought"
              ? item.id === liveThoughtId
              : busy && item.id === lastAgentProseId
          }
        />
      ))}
    </>
  );
}
