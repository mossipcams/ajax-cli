import { useState } from "react";
import type { ConversationItem } from "../session/public";
import { useActivityDisclosurePreference } from "./activityDisclosurePreference";
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

function toolItems(items: ConversationItem[]): ConversationItem[] {
  return items.filter((item) => item.kind === "tool");
}

function summaryTarget(items: ConversationItem[], live: boolean, expanded: boolean): string {
  if (live && !expanded) {
    return toolItems(items).length > 0 ? activitySummary(items) : currentOperation(items);
  }
  return activitySummary(items);
}

/** One disclosure per turn. Collapsed it always lists tool rows (bodies follow
 * ToolCard status); thoughts, plans and permission markers stay inside until
 * expanded. The summary is the counted line while tools are visible on a live
 * turn, otherwise the current operation so a thinking-only turn is not silent.
 * States that want the operator — a failure, an ask — open themselves, and a
 * tap in either direction sticks for the rest of the session. */
export default function TurnActivity({
  items,
  live,
  attention,
}: {
  items: ConversationItem[];
  live: boolean;
  attention: boolean;
}) {
  const { preference: sessionPreference, setPreference: setSessionPreference } =
    useActivityDisclosurePreference();
  const [turnOverride, setTurnOverride] = useState<boolean | null>(null);
  if (items.length === 0) return null;

  const failed = items.some((item) => item.kind === "tool" && item.call.status === "failed");
  const expanded =
    turnOverride ?? (failed || attention || (sessionPreference ?? false));
  const visibleItems = expanded ? items : toolItems(items);

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
        target={summaryTarget(items, live, expanded)}
        meta={expanded ? "⌃" : "⌄"}
        aria-expanded={expanded}
        onClick={() => {
          const next = !expanded;
          setTurnOverride(next);
          setSessionPreference(next);
        }}
      />
      {visibleItems.map((item) => (
        <WorkRow key={item.id} item={item} />
      ))}
    </div>
  );
}
