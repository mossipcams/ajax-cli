import type { ConversationItem } from "../session/public";
import Markdown from "./Markdown";
import { settledText } from "./reveal";

export default function AssistantTurn({
  item,
  live,
}: {
  item: ConversationItem;
  live: boolean;
}) {
  if (item.kind !== "prose" || item.role !== "agent") return null;
  const shown = live ? settledText(item.text) : item.text;
  if (!shown) return null;
  return (
    <article
      className="session-reply"
      data-testid="session-message-agent"
      {...(live ? { "data-live": "true" } : {})}
    >
      <Markdown source={shown} />
    </article>
  );
}
