import type { ConversationItem } from "../session/public";
import OutputContentBlockView from "./OutputContentBlockView";

export default function UserTurn({ item }: { item: ConversationItem }) {
  if (item.kind !== "prose" || item.role !== "user") return null;
  const blocks = item.contentBlocks ?? [];
  if (!item.text && blocks.length === 0) return null;
  return (
    <article className="session-said" data-testid="session-message-user">
      {item.text ? <span>{item.text}</span> : null}
      {blocks.map((block, index) => (
        <OutputContentBlockView key={index} block={block} />
      ))}
    </article>
  );
}
