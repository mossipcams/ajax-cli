// Session transcript: settled agent prose plus the live tail. Only the last
// agent row streams while busy; earlier rows stay settled so this list can
// hold scroll position. Tool traces stay out; a turn lands as prose plus at
// most one summary note.

import { memo } from "react";
import Markdown from "./Markdown";
import type { ThreadEntry } from "./sessionThread";

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
        <Markdown source={entry.text} smooth={live} />
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

/** Only the tail entry changes while a turn streams; every settled row above it
 * is referentially stable, so `memo` keeps a long transcript off the hot path. */
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
