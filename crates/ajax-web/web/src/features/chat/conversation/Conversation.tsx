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
            {turn.rows.map((row) =>
              row.kind === "work" ? (
                <TurnActivity
                  key={row.id}
                  items={row.items}
                  live={isLiveTurn}
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
