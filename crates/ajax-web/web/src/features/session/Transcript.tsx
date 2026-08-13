import { memo } from "react";
import Markdown from "./Markdown";
import type { ThreadEntry } from "./projectSession";

const Row = memo(function Row({
  entry,
  live,
}: {
  entry: ThreadEntry;
  live: boolean;
}) {
  if (entry.kind === "prose" && entry.role === "user") {
    return (
      <article className="session-said" data-testid="session-message-user">
        {entry.text}
      </article>
    );
  }

  if (entry.kind === "prose") {
    return (
      <article
        className={live ? "session-reply is-live" : "session-reply"}
        data-testid="session-message-agent"
        {...(live ? { "data-live": "true" } : {})}
      >
        <Markdown source={entry.text} />
      </article>
    );
  }

  return (
    <div
      className={`session-note tone-${entry.tone === "error" ? "error" : "muted"}`}
      data-testid={`session-note-${entry.tone}`}
    >
      <span className="session-note-text">{entry.text}</span>
    </div>
  );
});

export default function Transcript({
  entries,
  busy,
}: {
  entries: ThreadEntry[];
  busy: boolean;
}) {
  const lastAgentProseId = (() => {
    for (let i = entries.length - 1; i >= 0; i -= 1) {
      const entry = entries[i];
      if (entry.kind === "prose" && entry.role === "agent") return entry.id;
    }
    return null;
  })();

  return (
    <>
      {entries.map((entry) => (
        <Row
          key={entry.id}
          entry={entry}
          live={busy && entry.id === lastAgentProseId}
        />
      ))}
    </>
  );
}
