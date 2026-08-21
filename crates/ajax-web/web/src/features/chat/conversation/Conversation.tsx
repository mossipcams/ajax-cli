// Turn-as-chapter conversation: what the operator said, one line for what the
// agent did, what the agent answered.
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

import { TurnActivity } from "../activity/public";
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
  const lastAgentProseId = (() => {
    for (let i = items.length - 1; i >= 0; i -= 1) {
      const item = items[i];
      if (item.kind === "prose" && item.role === "agent") return item.id;
    }
    return null;
  })();

  const turns = groupConversationTurns(items);

  return (
    <>
      {turns.map((turn, turnIndex) => {
        const isLiveTurn = busy && turnIndex === turns.length - 1;
        const awaiting = turn.other.some(
          (item) => item.kind === "permission" && !item.resolved,
        );

        return (
          <div
            key={turn.id}
            className="session-turn"
            data-testid={turn.user ? "session-turn" : "session-turn-preamble"}
          >
            {turn.user ? <TranscriptRow item={turn.user} live={false} /> : null}
            <TurnActivity items={turn.work} live={isLiveTurn} attention={awaiting} />
            {turn.other.map((item) => (
              <TranscriptRow key={item.id} item={item} live={false} />
            ))}
            {turn.agents.map((item, index) => (
              <TranscriptRow
                key={item.id}
                item={item}
                live={
                  isLiveTurn && index === turn.agents.length - 1 && item.id === lastAgentProseId
                }
              />
            ))}
          </div>
        );
      })}
    </>
  );
}
