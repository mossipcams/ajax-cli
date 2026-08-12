// The settled transcript: what the agent has already said. Nothing streams
// here — live work lives in the head — so this list can hold its scroll
// position while the agent works. Tool traces stay out; a turn lands as prose
// plus at most one summary note.

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
    if (live) {
      return (
        <article
          className="session-reply is-live"
          data-testid="session-message-agent"
          data-live="true"
        >
          {entry.text}
        </article>
      );
    }
    return (
      <article className="session-reply" data-testid="session-message-agent">
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
