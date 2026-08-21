// Turn-as-chapter conversation: what the operator said, one line for what the
// agent did, what the agent answered.
//
// REVISION (mobile chat): the transcript is a conversation, not the ACP event
// stream. Thoughts, plans, tool calls, their output and their diffs are the
// substance of a turn but they are not the turn's message — they live behind
// one disclosure per turn and are reachable in one tap. What stays in the
// column is the operator's message, the agent's answer, an ask the operator
// still owes an answer to, an error, and a hairline divider for the events that
// changed the session out from under them.
//
// Reveal is by paragraph, never by token: a live answer shows the paragraphs it
// has finished and nothing else, so the column never reflows under a reader.

import { memo, useState } from "react";
import Markdown from "./Markdown";
import ToolCard, { ActivityRow } from "./ToolCard";
import { groupConversationTurns } from "./sessionTurns";
import {
  activePlanStep,
  thoughtSnippet,
  type ConversationItem,
  type PlanEntry,
  type ToolCall,
} from "./sessionThread";
import {
  cleanTitle,
  elapsedMs,
  formatElapsed,
  OPERATION_VERBS,
  toolTarget,
} from "./toolPresentation";

/** Complete paragraphs only. A partial sentence arriving word by word is the
 * protocol leaking into the conversation, so a live answer is cut back to its
 * last paragraph break — and never inside a fence, where the break is content. */
export function settledText(text: string): string {
  const cut = text.lastIndexOf("\n\n");
  if (cut < 0) return "";
  const head = text.slice(0, cut);
  if ((head.match(/```/g) ?? []).length % 2 === 0) return head;
  return head.slice(0, head.lastIndexOf("```")).trimEnd();
}

function Thought({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="session-thinking" data-testid="session-thinking">
      <ActivityRow
        className="session-thinking-toggle"
        mark="∴"
        tailChars={0}
        target={thoughtSnippet(text, 90)}
        aria-label={`Thinking — ${thoughtSnippet(text, 90)}`}
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      />
      {open ? (
        <p className="session-thinking-body" data-testid="session-thinking-body">
          {text}
        </p>
      ) : null}
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

function tools(items: ConversationItem[]): ToolCall[] {
  return items.flatMap((item) => (item.kind === "tool" ? [item.call] : []));
}

/** The one operation in flight. The card replaces this line rather than growing
 * a row per call: on a phone the log is the disclosure's job, not the turn's. */
export function currentOperation(items: ConversationItem[]): string {
  const running = tools(items)
    .filter((call) => call.status === "pending" || call.status === "in_progress")
    .pop();
  if (running) {
    return `${OPERATION_VERBS[running.kind] ?? "Working on"} ${toolTarget(running)}…`;
  }
  for (let i = items.length - 1; i >= 0; i -= 1) {
    const item = items[i];
    if (item.kind === "plan") {
      const step = activePlanStep(item.entries);
      if (step) return `${step}…`;
    }
    if (item.kind === "thought") return `${thoughtSnippet(item.text, 60)}`;
  }
  return "Working…";
}

function count(n: number, singular: string, plural: string): string {
  return `${n} ${n === 1 ? singular : plural}`;
}

/** What the turn did, once it is done doing it: "Read 6 files · edited 2 files
 * · ran 4 commands · 38s". Named work first, failures next, wall time last. */
export function activitySummary(items: ConversationItem[]): string {
  const calls = tools(items);
  const kinds = (...wanted: string[]) =>
    calls.filter((call) => wanted.includes(call.kind)).length;

  const parts: string[] = [];
  const read = kinds("read");
  const edited = kinds("edit", "move", "delete");
  const ran = kinds("execute");
  if (read) parts.push(`read ${count(read, "file", "files")}`);
  if (edited) parts.push(`edited ${count(edited, "file", "files")}`);
  if (ran) parts.push(`ran ${count(ran, "command", "commands")}`);

  if (!parts.length) {
    if (items.some((item) => item.kind === "plan")) parts.push("planning");
    else if (items.some((item) => item.kind === "thought")) parts.push("reasoning");
    else parts.push(count(items.length, "step", "steps"));
  }

  const failed = calls.filter((call) => call.status === "failed").length;
  if (failed) parts.push(`${failed} failed`);

  const first = calls.find((call) => call.startedAt !== undefined);
  const last = [...calls].reverse().find((call) => call.endedAt !== undefined);
  const span = formatElapsed(
    first && last ? elapsedMs({ startedAt: first.startedAt, endedAt: last.endedAt }) : undefined,
  );
  if (span) parts.push(span);

  const line = parts.join(" · ");
  return line.charAt(0).toUpperCase() + line.slice(1);
}

function WorkRow({ item }: { item: ConversationItem }) {
  switch (item.kind) {
    case "thought":
      return <Thought text={item.text} />;
    case "tool":
      return <ToolCard call={item.call} />;
    case "plan":
      return <PlanChecklist entries={item.entries} />;
    default:
      return <Row item={item} live={false} />;
  }
}

/** One disclosure per turn. Collapsed it is the current operation while the turn
 * runs and the summary once it settles; a completed call is inside the moment
 * ACP says so, because the collapsed row never lists calls at all. States that
 * want the operator — a failure, an ask — open themselves, and a tap in either
 * direction sticks for the rest of the session. */
function TurnActivity({
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

const Row = memo(function Row({ item, live }: { item: ConversationItem; live: boolean }) {
  switch (item.kind) {
    case "prose": {
      if (item.role === "user") {
        return (
          <article className="session-said" data-testid="session-message-user">
            {item.text}
          </article>
        );
      }
      // Paragraph-complete only while the turn runs; the whole answer once done.
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

    case "note":
      // An error is something to act on and keeps its own row; anything else
      // the host says happened to the session is a hairline divider.
      return item.tone === "error" ? (
        <div className="session-note tone-error" data-testid="session-note-error">
          <span className="session-note-text">{item.text}</span>
        </div>
      ) : (
        <p className="session-divider" data-testid="session-note-info">
          <span>{item.text}</span>
        </p>
      );
  }
});

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
        const awaiting = turn.other.some(
          (item) => item.kind === "permission" && !item.resolved,
        );

        return (
          <div
            key={turn.id}
            className="session-turn"
            data-testid={turn.user ? "session-turn" : "session-turn-preamble"}
          >
            {turn.user ? <Row item={turn.user} live={false} /> : null}
            <TurnActivity items={turn.work} live={isLiveTurn} attention={awaiting} />
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
