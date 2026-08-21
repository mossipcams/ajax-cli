import { useState } from "react";
import type { ConversationItem } from "../session/public";
import { activitySummary } from "./activitySummary";
import { currentOperation } from "./currentOperation";
import { cleanTitle } from "./presentation";
import PlanChecklist from "./PlanChecklist";
import Thought from "./Thought";
import ToolCard, { ActivityRow } from "./ToolCard";

function WorkRow({ item }: { item: ConversationItem }) {
  switch (item.kind) {
    case "thought":
      return <Thought text={item.text} />;
    case "tool":
      return <ToolCard call={item.call} />;
    case "plan":
      return <PlanChecklist entries={item.entries} />;
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
    default:
      return null;
  }
}

/** One disclosure per turn. Collapsed it is the current operation while the turn
 * runs and the summary once it settles; a completed call is inside the moment
 * ACP says so, because the collapsed row never lists calls at all. States that
 * want the operator — a failure, an ask — open themselves, and a tap in either
 * direction sticks for the rest of the session. */
export default function TurnActivity({
  items,
  live,
  attention,
}: {
  items: ConversationItem[];
  live: boolean;
  attention: boolean;
}) {
  const [open, setOpen] = useState<boolean | null>(null);
  if (items.length === 0) return null;

  const failed = items.some((item) => item.kind === "tool" && item.call.status === "failed");
  const expanded = open ?? (failed || attention);

  return (
    <div
      className={`session-turn-work${failed ? " has-failure" : ""}`}
      data-testid="session-turn-work"
      data-expanded={expanded ? "true" : "false"}
      data-live={live ? "true" : undefined}
    >
      <ActivityRow
        className="session-turn-work-summary"
        data-testid="session-turn-work-summary"
        mark="⚙"
        tailChars={0}
        target={live && !expanded ? currentOperation(items) : activitySummary(items)}
        meta={expanded ? "⌃" : "⌄"}
        aria-expanded={expanded}
        onClick={() => setOpen(!expanded)}
      />
      {expanded ? items.map((item) => <WorkRow key={item.id} item={item} />) : null}
    </div>
  );
}
