import type { ConversationItem } from "../session/public";

export default function UserTurn({ item }: { item: ConversationItem }) {
  if (item.kind !== "prose" || item.role !== "user") return null;
  return (
    <article className="session-said" data-testid="session-message-user">
      {item.text}
    </article>
  );
}
