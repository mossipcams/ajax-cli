import { memo } from "react";
import { cleanTitle } from "../activity/public";
import type { ConversationItem } from "../session/public";
import AssistantTurn from "./AssistantTurn";
import UserTurn from "./UserTurn";

const TranscriptRow = memo(function TranscriptRow({
  item,
  live,
}: {
  item: ConversationItem;
  live: boolean;
}) {
  switch (item.kind) {
    case "prose":
      if (item.role === "user") return <UserTurn item={item} />;
      return <AssistantTurn item={item} live={live} />;

    case "permission":
      return (
        <div
          className={`session-note tone-${item.resolved ? "muted" : "waiting"}`}
          data-testid="session-permission-marker"
          data-resolved={item.resolved ? "true" : "false"}
        >
          <span className="session-note-label">
            {item.resolved ? "Answered" : "Permission requested"}
          </span>
          <span className="session-note-text">{cleanTitle(item.title)}</span>
        </div>
      );

    case "elicitation":
      return (
        <div
          className={`session-note tone-${item.resolved ? "muted" : "waiting"}`}
          data-testid="session-elicitation-marker"
          data-resolved={item.resolved ? "true" : "false"}
        >
          <span className="session-note-label">
            {item.resolved ? "Answered" : "Agent request"}
          </span>
          <span className="session-note-text">{item.message}</span>
        </div>
      );

    case "note":
      return item.tone === "error" ? (
        <div className="session-note tone-error" data-testid="session-note-error">
          <span className="session-note-text">{item.text}</span>
        </div>
      ) : (
        <p className="session-divider" data-testid="session-note-info">
          <span>{item.text}</span>
        </p>
      );

    default:
      return null;
  }
});

export default TranscriptRow;
