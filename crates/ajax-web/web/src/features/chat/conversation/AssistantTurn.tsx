import type { ConversationItem } from "../session/public";
import Markdown from "./Markdown";
import OutputContentBlockView from "./OutputContentBlockView";
import ProseCopyButton from "./ProseCopyButton";
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
  const blocks = item.contentBlocks ?? [];
  const pending = live && item.text.length > shown.length;
  if (!shown && blocks.length === 0 && !pending) return null;
  return (
    <article
      className="session-reply"
      data-testid="session-message-agent"
      {...(live ? { "data-live": "true" } : {})}
    >
      {shown ? <Markdown source={shown} /> : null}
      {blocks.map((block, index) => (
        <OutputContentBlockView key={index} block={block} />
      ))}
      {pending ? (
        <span
          className="session-reply-pending"
          data-testid="session-reply-pending"
          aria-hidden="true"
        />
      ) : null}
      {!live && item.text ? <ProseCopyButton text={item.text} /> : null}
    </article>
  );
}
