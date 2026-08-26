// Turn-as-chapter conversation: what the operator said, one line for what the
// agent did, what the agent answered — in the order those things happened. An
// agent that speaks, works, then speaks again reads that way; hoisting the work
// above everything it said broke the one thing a transcript owes a reader.
//
// REVISION (mobile chat): the transcript is a conversation, not the ACP event
// stream. Thoughts, plans, tool calls, their output and their diffs are the
// substance of a turn but they are not the turn's message — they live behind
// one disclosure per turn and are reachable in one tap. What stays in the
// column is the operator's message, the agent's answer, an ask the operator
// still owes an answer to, an error, and a hairline divider for the events that
// changed the session out from under them.
//
// Reveal is by paragraph, never by token: a live answer shows the paragraphs it
// has finished and nothing else, so the column never reflows under a reader.

import {
  ActivityDisclosurePreferenceProvider,
  TurnActivity,
} from "../activity/public";
import type { ConversationItem } from "../session/public";
import { groupConversationTurns } from "./groupTurns";
import TranscriptRow from "./Turn";

export default function Conversation({
  items,
  busy,
}: {
  items: ConversationItem[];
  busy: boolean;
}) {
  // Only the row still being written is held back to completed paragraphs.
  // Anything the turn has already moved past — a tool call, an ask, a later
  // message — is proof the message before it finished, and a one-paragraph
  // "Let me look at the handler." has no paragraph break to wait for, so
  // gating it on one hid it for the whole turn.
  const streamingProseId = (() => {
    const last = items[items.length - 1];
    if (!last || last.kind !== "prose" || last.role !== "agent") return null;
    return last.id;
  })();

  const turns = groupConversationTurns(items);

  return (
    <ActivityDisclosurePreferenceProvider>
      {turns.map((turn, turnIndex) => {
        const isLiveTurn = busy && turnIndex === turns.length - 1;
        const awaiting = turn.rows.some(
          (row) => row.kind === "item" && row.item.kind === "permission" && !row.item.resolved,
        );

        return (
          <div
            key={turn.id}
            className="session-turn"
            data-testid={turn.user ? "session-turn" : "session-turn-preamble"}
          >
            {turn.user ? <TranscriptRow item={turn.user} live={false} /> : null}
            {turn.rows.map((row, rowIndex) =>
              row.kind === "work" ? (
                <TurnActivity
                  key={row.id}
                  items={row.items}
                  live={isLiveTurn && rowIndex === turn.rows.length - 1}
                  attention={awaiting}
                />
              ) : (
                <TranscriptRow
                  key={row.id}
                  item={row.item}
                  live={isLiveTurn && row.item.id === streamingProseId}
                />
              ),
            )}
          </div>
        );
      })}
    </ActivityDisclosurePreferenceProvider>
  );
}
