// Agent prose is markdown. This renders the six constructs agents actually
// emit into React nodes — fenced code, inline code, headings, bullet and
// ordered lists, bold — and lets everything else through as text.
//
// Deliberately not a markdown library: the full CommonMark surface is not
// reachable from a chat turn, and a parser that never touches innerHTML cannot
// inject agent output into the DOM as markup.

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";

type Block =
  | { kind: "code"; lang: string; text: string }
  | { kind: "heading"; text: string }
  | { kind: "list"; ordered: boolean; items: string[] }
  | { kind: "para"; text: string };

const FENCE = /^```(\w*)\s*$/;
const HEADING = /^#{1,6}\s+(.*)$/;
const BULLET = /^\s*[-*+]\s+(.*)$/;
const ORDERED = /^\s*\d+[.)]\s+(.*)$/;

export function parseBlocks(source: string): Block[] {
  const lines = source.split("\n");
  const blocks: Block[] = [];
  let paragraph: string[] = [];

  const flush = () => {
    const text = paragraph.join("\n").trim();
    if (text) blocks.push({ kind: "para", text });
    paragraph = [];
  };

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    const fence = FENCE.exec(line);
    if (fence) {
      flush();
      const body: string[] = [];
      i += 1;
      while (i < lines.length && !FENCE.test(lines[i])) {
        body.push(lines[i]);
        i += 1;
      }
      blocks.push({ kind: "code", lang: fence[1], text: body.join("\n") });
      continue;
    }

    const heading = HEADING.exec(line);
    if (heading) {
      flush();
      blocks.push({ kind: "heading", text: heading[1] });
      continue;
    }

    const bullet = BULLET.exec(line);
    const ordered = bullet ? null : ORDERED.exec(line);
    if (bullet || ordered) {
      const isOrdered = Boolean(ordered);
      const tail = blocks[blocks.length - 1];
      const item = (bullet ?? ordered)![1];
      if (paragraph.length) flush();
      if (tail?.kind === "list" && tail.ordered === isOrdered) tail.items.push(item);
      else blocks.push({ kind: "list", ordered: isOrdered, items: [item] });
      continue;
    }

    if (!line.trim()) flush();
    else paragraph.push(line);
  }
  flush();
  return blocks;
}

const INLINE = /(`[^`]+`|\*\*[^*]+\*\*)/g;

/** Inline code and bold only. Anything else stays literal text. */
export function renderInline(text: string, keyPrefix: string): ReactNode[] {
  return text.split(INLINE).map((part, index) => {
    const key = `${keyPrefix}-${index}`;
    if (part.startsWith("`") && part.endsWith("`") && part.length > 2) {
      return (
        <code key={key} className="md-code">
          {part.slice(1, -1)}
        </code>
      );
    }
    if (part.startsWith("**") && part.endsWith("**") && part.length > 4) {
      return <strong key={key}>{part.slice(2, -2)}</strong>;
    }
    return part;
  });
}

/** A streaming turn delivers a chunk per token, and each one would otherwise
 * re-parse and re-render the whole message. 50ms is under the ~100ms at which a
 * redraw stops reading as continuous, so the text still streams; it just stops
 * costing a full parse per token on a phone.
 *
 * Leading edge plus trailing timer: the first chunk paints immediately, and the
 * last one is never stranded waiting for a chunk that will not come. */
const THROTTLE_MS = 50;

function useThrottledSource(source: string, live: boolean): string {
  const [shown, setShown] = useState(source);
  const lastAtRef = useRef(0);

  useEffect(() => {
    if (!live) {
      setShown(source);
      return;
    }
    const elapsed = Date.now() - lastAtRef.current;
    if (elapsed >= THROTTLE_MS) {
      lastAtRef.current = Date.now();
      setShown(source);
      return;
    }
    const timer = window.setTimeout(() => {
      lastAtRef.current = Date.now();
      setShown(source);
    }, THROTTLE_MS - elapsed);
    return () => window.clearTimeout(timer);
  }, [source, live]);

  return shown;
}

export default function Markdown({ source, live = false }: { source: string; live?: boolean }) {
  const shown = useThrottledSource(source, live);
  const blocks = useMemo(() => parseBlocks(shown), [shown]);
  return (
    <div className="md">
      {blocks.map((block, index) => {
        const key = `b${index}`;
        if (block.kind === "code") {
          return (
            <pre key={key} className="md-block" data-lang={block.lang || undefined}>
              <code>{block.text}</code>
            </pre>
          );
        }
        if (block.kind === "heading") {
          return (
            <h3 key={key} className="md-heading">
              {renderInline(block.text, key)}
            </h3>
          );
        }
        if (block.kind === "list") {
          const Tag = block.ordered ? "ol" : "ul";
          return (
            <Tag key={key} className="md-list">
              {block.items.map((item, itemIndex) => (
                <li key={`${key}-${itemIndex}`}>{renderInline(item, `${key}-${itemIndex}`)}</li>
              ))}
            </Tag>
          );
        }
        return (
          <p key={key} className="md-para">
            {renderInline(block.text, key)}
          </p>
        );
      })}
    </div>
  );
}
