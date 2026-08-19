// Turn-as-chapter conversation: operator bubble, collapsed work, agent answer.
//
// Only the last agent row streams while busy; earlier rows stay settled so this
// list can hold scroll position.

import { memo, useState } from "react";
import Markdown from "./Markdown";
import ToolCard, { ActivityRow } from "./ToolCard";
import { flattenTurnItems, groupConversationTurns } from "./sessionTurns";
import { thoughtSnippet, type ConversationItem, type PlanEntry } from "./sessionThread";
import { cleanTitle, elapsedMs, formatElapsed } from "./toolPresentation";

function Thinking({ text, live }: { text: string; live: boolean }) {
  const [manualOpen, setManualOpen] = useState(false);
  const expanded = live || manualOpen;
  return (
    <div className="session-thinking" data-testid="session-thinking">
      <ActivityRow
        className="session-thinking-toggle"
        mark="∴"
        tailChars={0}
        target={thoughtSnippet(text, 90)}
        aria-label={`Thinking — ${thoughtSnippet(text, 90)}`}
        aria-expanded={expanded}
        onClick={() => setManualOpen(!manualOpen)}
      />
      {expanded ? (
        <p className="session-thinking-body" data-testid="session-thinking-body">
          {text}
        </p>
      ) : null}
    </div>
  );
}

const ACTIVITY_KINDS = ["tool", "thought"];

function isActivity(item: ConversationItem): boolean {
  return ACTIVITY_KINDS.includes(item.kind);
}

export function activityRuns(items: ConversationItem[]): ConversationItem[][] {
  const runs: ConversationItem[][] = [];
  for (const item of items) {
    const last = runs[runs.length - 1];
    if (last && isActivity(item) && isActivity(last[0])) last.push(item);
    else runs.push([item]);
  }
  return runs;
}

function runSummary(items: ConversationItem[]): string {
  const calls = items.flatMap((item) => (item.kind === "tool" ? [item.call] : []));
  const failed = calls.filter((call) => call.status === "failed").length;
  const parts = [
    calls.length
      ? `${calls.length} ${calls.length === 1 ? "tool" : "tools"}`
      : `${items.length} thoughts`,
  ];
  if (failed) parts.push(`${failed} failed`);
  const first = calls.find((call) => call.startedAt !== undefined);
  const last = [...calls].reverse().find((call) => call.endedAt !== undefined);
  const span = formatElapsed(
    first && last ? elapsedMs({ startedAt: first.startedAt, endedAt: last.endedAt }) : undefined,
  );
  if (span) parts.push(span);
  return parts.join(" · ");
}

function isLiveTail(items: ConversationItem[], item: ConversationItem): boolean {
  return items[items.length - 1]?.id === item.id;
}

function ActivityRun({ items, live }: { items: ConversationItem[]; live: boolean }) {
  const [open, setOpen] = useState<boolean | null>(null);
  const unsettled = items.some(
    (item) => item.kind === "tool" && item.call.status !== "completed",
  );
  const expanded = open ?? (live || unsettled);
  const summarised = items.length > 1;
  const failed = items.some((item) => item.kind === "tool" && item.call.status === "failed");

  return (
    <div
      className={`session-activity${failed ? " has-failure" : ""}`}
      data-testid="session-activity"
      data-expanded={expanded ? "true" : "false"}
    >
      {summarised ? (
        <ActivityRow
          className="session-activity-summary"
          data-testid="session-activity-summary"
          mark="⚙"
          target={runSummary(items)}
          meta={expanded ? "⌃" : "⌄"}
          aria-expanded={expanded}
          onClick={() => setOpen(!expanded)}
        />
      ) : null}
      {!summarised || expanded
        ? items.map((item) => (
            <Row key={item.id} item={item} live={live && isLiveTail(items, item)} />
          ))
        : null}
    </div>
  );
}

const PLAN_MARKS: Record<string, string> = {
  completed: "✓",
  in_progress: "▸",
  pending: "·",
};

function PlanChecklist({ entries }: { entries: PlanEntry[] }) {
  return (
    <ol className="session-plan" data-testid="session-plan">
      {entries.map((entry, index) => (
        <li
          key={`${index}-${entry.content}`}
          className="session-plan-step"
          data-status={entry.status}
        >
          <span className="session-plan-mark" aria-hidden="true">
            {PLAN_MARKS[entry.status] ?? "·"}
          </span>
          <span className="session-plan-text">{entry.content}</span>
        </li>
      ))}
    </ol>
  );
}

const Row = memo(function Row({ item, live }: { item: ConversationItem; live: boolean }) {
  switch (item.kind) {
    case "prose":
      return item.role === "user" ? (
        <article className="session-said" data-testid="session-message-user">
          {item.text}
        </article>
      ) : (
        <article
          className={live ? "session-reply is-live" : "session-reply"}
          data-testid="session-message-agent"
          {...(live ? { "data-live": "true" } : {})}
        >
          <Markdown source={item.text} live={live} />
        </article>
      );

    case "thought":
      return <Thinking text={item.text} live={live} />;

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

    case "note":
      return (
        <div
          className={`session-note tone-${item.tone === "error" ? "error" : "muted"}`}
          data-testid={`session-note-${item.tone}`}
        >
          <span className="session-note-text">{item.text}</span>
        </div>
      );
  }
});

function workSummary(items: ConversationItem[]): string {
  const calls = items.flatMap((item) => (item.kind === "tool" ? [item.call] : []));
  const edits = calls.filter((call) => call.kind === "edit").length;
  const executes = calls.filter((call) => call.kind === "execute").length;
  const failed = calls.filter((call) => call.status === "failed").length;
  const parts: string[] = [];

  if (edits) parts.push(`Edited ${edits} ${edits === 1 ? "file" : "files"}`);
  if (executes) parts.push(executes === 1 ? "ran command" : `ran ${executes} commands`);
  if (!edits && !executes) {
    if (calls.length) parts.push(`${calls.length} ${calls.length === 1 ? "tool" : "tools"}`);
    else {
      const thoughts = items.filter((item) => item.kind === "thought").length;
      if (thoughts) parts.push(`${thoughts} ${thoughts === 1 ? "thought" : "thoughts"}`);
    }
  }
  if (failed) parts.push(`${failed} failed`);

  const first = calls.find((call) => call.startedAt !== undefined);
  const last = [...calls].reverse().find((call) => call.endedAt !== undefined);
  const span = formatElapsed(
    first && last ? elapsedMs({ startedAt: first.startedAt, endedAt: last.endedAt }) : undefined,
  );
  if (span) parts.push(span);

  if (parts.length === 0) {
    const plans = items.some((item) => item.kind === "plan");
    const permissions = items.some((item) => item.kind === "permission");
    if (plans && permissions) return "Plan · permission";
    if (plans) return "Plan";
    if (permissions) return "Permission";
    return `${items.length} steps`;
  }
  return parts.join(" · ");
}

function WorkRow({ item, live }: { item: ConversationItem; live: boolean }) {
  switch (item.kind) {
    case "thought":
      return <Thinking text={item.text} live={live} />;
    case "tool":
      return <ToolCard call={item.call} />;
    case "plan":
      return <PlanChecklist entries={item.entries} />;
    case "permission":
      return <Row item={item} live={false} />;
    default:
      return <Row item={item} live={false} />;
  }
}

function TurnChapter({ items, live }: { items: ConversationItem[]; live: boolean }) {
  const [open, setOpen] = useState<boolean | null>(null);
  if (items.length === 0) return null;

  const unsettled = items.some(
    (item) => item.kind === "tool" && item.call.status !== "completed",
  );
  const failed = items.some((item) => item.kind === "tool" && item.call.status === "failed");
  const expanded = open ?? (live || unsettled);
  const summarised = items.length > 1 || (items.length === 1 && items[0].kind !== "tool");

  if (!summarised) {
    return <WorkRow item={items[0]} live={live} />;
  }

  return (
    <div
      className={`session-turn-work${failed ? " has-failure" : ""}`}
      data-testid="session-turn-work"
      data-expanded={expanded ? "true" : "false"}
    >
      <ActivityRow
        className="session-turn-work-summary"
        data-testid="session-turn-work-summary"
        mark="⚙"
        target={workSummary(items)}
        meta={expanded ? "⌃" : "⌄"}
        aria-expanded={expanded}
        onClick={() => setOpen(!expanded)}
      />
      {expanded
        ? items.map((item, index) => (
            <WorkRow
              key={item.id}
              item={item}
              live={live && index === items.length - 1 && item.kind === "thought"}
            />
          ))
        : null}
    </div>
  );
}

function renderLegacySegment(
  items: ConversationItem[],
  busy: boolean,
  lastAgentProseId: string | null,
) {
  const runs = activityRuns(items);
  return runs.map((run, index) =>
    isActivity(run[0]) ? (
      <ActivityRun key={run[0].id} items={run} live={busy && index === runs.length - 1} />
    ) : (
      <Row key={run[0].id} item={run[0]} live={busy && run[0].id === lastAgentProseId} />
    ),
  );
}

export default function Transcript({
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

        if (!turn.user) {
          return (
            <div key={turn.id} className="session-turn" data-testid="session-turn-preamble">
              {renderLegacySegment(flattenTurnItems(turn), busy && isLiveTurn, lastAgentProseId)}
            </div>
          );
        }

        const lastWork = turn.work[turn.work.length - 1];
        const workLive = isLiveTurn && lastWork?.kind === "thought";

        return (
          <div key={turn.id} className="session-turn" data-testid="session-turn">
            <Row item={turn.user} live={false} />
            <TurnChapter items={turn.work} live={workLive} />
            {turn.other.map((item) => (
              <Row key={item.id} item={item} live={false} />
            ))}
            {turn.agents.map((item, index) => (
              <Row
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

export { workSummary };
