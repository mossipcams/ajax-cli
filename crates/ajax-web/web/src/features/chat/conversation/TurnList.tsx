// The transcript as a list of turns rather than a stream of messages.
//
// Supervising work is not chatting: the operator arrives at a task that ran
// while they were away, and the questions are "did it finish", "does it need
// me", "what did it conclude", "what did it change" — in that order. A stream
// of equal-weight messages answers the last question you ask and buries the
// first four, so a settled turn collapses to its prompt and its outcome, and
// the turn you came back for is the one that is open.

import { useState } from "react";
import { TurnActivity } from "../activity/public";
import type { ConversationItem } from "../session/public";
import { groupConversationTurns, type ConversationTurn } from "./groupTurns";
import { opensByDefault, turnDigest, type TurnDigest } from "./turnDigest";
import TranscriptRow from "./Turn";

function ChangedFiles({ digest }: { digest: TurnDigest }) {
  if (!digest.changed.length) return null;
  return (
    <ul className="session-turn-changed" data-testid="session-turn-changed">
      {digest.changed.map((file) => (
        <li key={file.path}>
          <span className="session-turn-changed-path">{file.path.split("/").pop()}</span>
          {file.added ? <span className="session-turn-added">+{file.added}</span> : null}
          {file.removed ? <span className="session-turn-removed">−{file.removed}</span> : null}
        </li>
      ))}
    </ul>
  );
}

function TurnBody({
  turn,
  digest,
  live,
  streamingProseId,
}: {
  turn: ConversationTurn;
  digest: TurnDigest;
  live: boolean;
  streamingProseId: string | null;
}) {
  return (
    <>
      {turn.rows.map((row, index) =>
        row.kind === "work" ? (
          <TurnActivity
            key={row.id}
            items={row.items}
            live={live && index === turn.rows.length - 1}
            attention={digest.awaiting}
          />
        ) : (
          <TranscriptRow
            key={row.id}
            item={row.item}
            live={live && row.item.id === streamingProseId}
          />
        ),
      )}
      <ChangedFiles digest={digest} />
    </>
  );
}

function Turn({
  turn,
  live,
  isLast,
  streamingProseId,
}: {
  turn: ConversationTurn;
  live: boolean;
  isLast: boolean;
  streamingProseId: string | null;
}) {
  const digest = turnDigest(turn);
  const [open, setOpen] = useState<boolean | null>(null);
  const expanded = open ?? opensByDefault(digest, { isLast, isLive: live });
  const mark = digest.failed ? "✕" : live ? "▸" : digest.awaiting ? "?" : "✓";

  return (
    <section
      className={`session-turn-card${expanded ? " is-open" : ""}`}
      data-testid="session-turn"
      data-open={expanded ? "true" : "false"}
      data-failed={digest.failed ? "true" : undefined}
    >
      {digest.ask ? (
        <button
          type="button"
          className="session-turn-ask"
          data-testid="session-turn-ask"
          aria-expanded={expanded}
          onClick={() => setOpen(!expanded)}
        >
          <span
            className={`session-turn-mark${digest.failed ? " is-failed" : ""}${live ? " is-live" : ""}`}
            aria-hidden="true"
          >
            {mark}
          </span>
          <span className="session-turn-ask-text">{digest.ask}</span>
        </button>
      ) : null}

      {expanded ? (
        <TurnBody
          turn={turn}
          digest={digest}
          live={live}
          streamingProseId={streamingProseId}
        />
      ) : digest.outcome ? (
        <p className="session-turn-outcome" data-testid="session-turn-outcome">
          {digest.outcome}
        </p>
      ) : null}
    </section>
  );
}

export default function TurnList({
  items,
  busy,
}: {
  items: ConversationItem[];
  busy: boolean;
}) {
  const last = items[items.length - 1];
  const streamingProseId =
    last && last.kind === "prose" && last.role === "agent" ? last.id : null;
  const turns = groupConversationTurns(items);

  return (
    <>
      {turns.map((turn, index) => {
        const isLast = index === turns.length - 1;
        return (
          <Turn
            key={turn.id}
            turn={turn}
            live={busy && isLast}
            isLast={isLast}
            streamingProseId={streamingProseId}
          />
        );
      })}
    </>
  );
}
