// The conversation: everything ACP sent, in the order it sent it. Prose,
// reasoning, tool calls with their output, the plan, and a marker where the
// agent asked for permission.
//
// Only the last agent row streams while busy; earlier rows stay settled so this
// list can hold scroll position.

import { memo, useState } from "react";
import Markdown from "./Markdown";
import ToolCard, { ActivityRow } from "./ToolCard";
import { thoughtSnippet, type ConversationItem, type PlanEntry } from "./sessionThread";
import { elapsedMs, formatElapsed } from "./toolPresentation";

/** Reasoning is the agent's account of its own turn: worth keeping, never worth
 * outranking the answer. Collapsed to one line until asked for, except while it
 * is the live tail of a busy turn.
 *
 * The collapsed line carries the reasoning itself rather than the word
 * "Thinking": a divider that only announces a section spends a full row of a
 * phone on no information. */
function Thinking({ text, live }: { text: string; live: boolean }) {
  const [manualOpen, setManualOpen] = useState(false);
  const expanded = live || manualOpen;
  return (
    <div className="session-thinking" data-testid="session-thinking">
      <ActivityRow
        className="session-thinking-toggle"
        mark="∴"
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

/** Split the conversation into runs: each stretch of tool calls and reasoning
 * between two pieces of prose is one unit of work, and everything else stands
 * alone. */
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
  // Wall time across the run, not the sum of its calls: the gaps between them
  // are the agent thinking, and the operator waited through those too.
  const first = calls.find((call) => call.startedAt !== undefined);
  const last = [...calls].reverse().find((call) => call.endedAt !== undefined);
  const span = formatElapsed(
    first && last ? elapsedMs({ startedAt: first.startedAt, endedAt: last.endedAt }) : undefined,
  );
  if (span) parts.push(span);
  return parts.join(" · ");
}

/** A finished run of work collapses to one row. The operator reads a
 * conversation and opens the work when they want it; while it is still running,
 * or when something in it failed, it is already open. */
function ActivityRun({ items, live }: { items: ConversationItem[]; live: boolean }) {
  const [open, setOpen] = useState<boolean | null>(null);
  const unsettled = items.some(
    (item) => item.kind === "tool" && item.call.status !== "completed",
  );
  const expanded = open ?? (live || unsettled);
  // One row summarising one row is worse than the row.
  const summarised = items.length > 1;

  return (
    <div
      className="session-activity"
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
        ? items.map((item) => <Row key={item.id} item={item} live={live && isLiveTail(items, item)} />)
        : null}
    </div>
  );
}

/** Only the run's last row can be the live one. */
function isLiveTail(items: ConversationItem[], item: ConversationItem): boolean {
  return items[items.length - 1]?.id === item.id;
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

    // The buttons are in the head, which cannot scroll away. This row is the
    // history: it says the agent asked here, and how it ended.
    case "permission":
      return (
        <div
          className={`session-note tone-${item.resolved ? "muted" : "waiting"}`}
          data-testid="session-permission-marker"
          data-resolved={item.resolved ? "true" : "false"}
        >
          <span className="session-note-text">
            {item.resolved ? `Answered · ${item.title}` : `Permission requested · ${item.title}`}
          </span>
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

/** Only the tail item changes while a turn streams; every settled row above it
 * is referentially stable, so `memo` keeps a long conversation off the hot path. */
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
  const runs = activityRuns(items);

  return (
    <>
      {runs.map((run, index) =>
        isActivity(run[0]) ? (
          <ActivityRun key={run[0].id} items={run} live={busy && index === runs.length - 1} />
        ) : (
          <Row key={run[0].id} item={run[0]} live={busy && run[0].id === lastAgentProseId} />
        ),
      )}
    </>
  );
}
