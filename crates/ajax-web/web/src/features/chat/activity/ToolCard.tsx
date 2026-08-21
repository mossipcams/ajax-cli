// A tool call in the conversation: one row on the shared activity grid — mark,
// target, elapsed — over the body holding what the call actually produced, the
// text a command printed or the diff an edit wrote.
//
// The row states the target, not the tool: "Read File" is identical on every
// read, and the path is the only reason to look. Success spends no words — the
// mark carries it — so the right column is free for the states that want the
// operator.
//
// Default collapse follows status, not preference: a call that succeeded needs
// one line, a call that failed or is still running is why the operator opened
// this surface. Once toggled, the operator's choice wins for that call.

import { useState } from "react";
import type { ToolCall, ToolContent } from "../session/public";
import OutputContentBlockView from "../conversation/OutputContentBlockView";
import {
  cleanTitle,
  CONTENT_PREVIEW_LINES,
  diffLines,
  elapsedMs,
  formatElapsed,
  middleSplit,
  shortPath,
  textPreview,
  toolMark,
  toolRowLabel,
  toolStatusNote,
  toolTarget,
  TOOL_TONES,
} from "./presentation";

/** The one row shape every activity line uses: mark, target, right-hand meta.
 * One grid, so a column of them reads as a column and not as loose paragraphs. */
export function ActivityRow({
  mark,
  target,
  meta,
  tailChars,
  mono,
  ...rest
}: {
  mark: string;
  target: string;
  meta?: string | null;
  /** Characters held back from ellipsis. Paths and commands are distinguished by
   * their end, so they keep the default; prose is not, and passes 0 rather than
   * ending on a severed word. */
  tailChars?: number;
  /** The target is a path, a command, or other machine text. Labels and prose
   * are set in the body face like the rest of the conversation. */
  mono?: boolean;
} & React.ComponentProps<"button">) {
  const [head, tail] = middleSplit(target, tailChars);
  return (
    <button type="button" {...rest} className={`session-row ${rest.className ?? ""}`}>
      <span className="session-row-mark" aria-hidden="true">
        {mark}
      </span>
      <span className={mono ? "session-row-target is-mono" : "session-row-target"}>
        <span className="session-row-head">{head}</span>
        {tail ? <span className="session-row-tail">{tail}</span> : null}
      </span>
      {meta ? <span className="session-row-meta">{meta}</span> : null}
    </button>
  );
}

function DiffBlock({ path, oldText, newText }: { path: string; oldText: string; newText: string }) {
  const lines = diffLines(oldText, newText);
  const added = lines.filter((line) => line.sign === "+").length;
  const removed = lines.filter((line) => line.sign === "-").length;
  return (
    <figure className="session-diff" data-testid="session-tool-diff">
      <figcaption className="session-diff-head">
        <span className="session-diff-path" title={path}>
          {shortPath(path)}
        </span>
        <span className="session-diff-stat">
          <span className="session-diff-added">+{added}</span>{" "}
          <span className="session-diff-removed">−{removed}</span>
        </span>
      </figcaption>
      <pre className="session-diff-body">
        {lines.map((line, index) => (
          <span key={index} className={`session-diff-line sign-${signClass(line.sign)}`}>
            {line.sign}
            {line.text}
            {"\n"}
          </span>
        ))}
      </pre>
    </figure>
  );
}

function signClass(sign: " " | "-" | "+"): string {
  if (sign === "+") return "add";
  if (sign === "-") return "del";
  return "same";
}

type OutputBlockKind = "search" | "read" | "output";

function outputBlockKind(kind: string): OutputBlockKind {
  if (kind === "search") return "search";
  if (kind === "read") return "read";
  return "output";
}

function TextOutputBlock({
  text,
  failed,
  blockKind,
}: {
  text: string;
  failed: boolean;
  blockKind: OutputBlockKind;
}) {
  const [showAll, setShowAll] = useState(false);
  const { preview, hiddenLines } = textPreview(text, CONTENT_PREVIEW_LINES, failed);
  const truncated = hiddenLines > 0 && !showAll;

  return (
    <>
      <pre
        className={`session-tool-output session-block-${blockKind}`}
        data-testid="session-tool-output"
        data-block-kind={blockKind}
      >
        {truncated ? preview : text}
      </pre>
      {hiddenLines > 0 ? (
        <button
          type="button"
          className="session-tool-output-expand"
          data-testid="session-tool-output-expand"
          onClick={() => setShowAll(!showAll)}
        >
          {showAll ? "Show less" : `${hiddenLines} more line${hiddenLines === 1 ? "" : "s"}`}
        </button>
      ) : null}
    </>
  );
}

function ContentBlock({
  content,
  failed,
  kind,
}: {
  content: ToolContent;
  failed: boolean;
  kind: string;
}) {
  if (content.type === "diff") {
    return (
      <DiffBlock
        path={content.path}
        oldText={content.oldText ?? ""}
        newText={content.newText}
      />
    );
  }
  if (content.type === "image" || content.type === "resource_link" || content.type === "resource") {
    return <OutputContentBlockView block={content} />;
  }
  // Execute output arrives here as text: Ajax advertises no `terminal/*` client
  // capability, so there is never an embedded terminal to render instead.
  return (
    <TextOutputBlock text={content.text} failed={failed} blockKind={outputBlockKind(kind)} />
  );
}

function defaultExpanded(call: ToolCall): boolean {
  if (call.status === "completed") return false;
  return call.content.length > 0;
}

export default function ToolCard({ call }: { call: ToolCall }) {
  const [open, setOpen] = useState<boolean | null>(null);
  const expanded = (open ?? defaultExpanded(call)) && call.content.length > 0;
  const tone = TOOL_TONES[call.kind] ?? "muted";
  const rowLabel = toolRowLabel(call);
  const target = toolTarget(call);
  const label = cleanTitle(call.title) || target;
  const failed = call.status === "failed";

  return (
    <section
      className={`session-toolcard tone-${tone}`}
      data-testid="session-tool-card"
      data-status={call.status}
      data-kind={call.kind || "other"}
    >
      <ActivityRow
        className="session-toolcard-head"
        mark={toolMark(call.kind)}
        target={rowLabel}
        mono
        // Elapsed is the resting right column; a word replaces it only when the
        // call is running, queued, or broken.
        meta={toolStatusNote(call.status) ?? formatElapsed(elapsedMs(call))}
        // The row shows the action; the accessible name keeps what ran.
        aria-label={rowLabel === label ? label : `${rowLabel} · ${target}`}
        title={call.locations[0] ?? label}
        // Nothing to show, nothing to toggle — but the row still states what ran.
        disabled={call.content.length === 0}
        aria-expanded={expanded}
        onClick={() => setOpen(!expanded)}
      />

      {expanded ? (
        <div
          className={`session-toolcard-body${failed ? " is-failure" : ""}`}
          {...(failed ? { "data-testid": "session-tool-failure-body" } : {})}
        >
          {call.content.map((content, index) => (
            <ContentBlock key={index} content={content} failed={failed} kind={call.kind} />
          ))}
        </div>
      ) : null}
    </section>
  );
}
