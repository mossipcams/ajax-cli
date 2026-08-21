// Agent prose is markdown. This renders the constructs agents actually emit into
// React nodes — fenced code, inline code, headings, lists (including nested),
// tables, blockquotes, links, bold — and lets everything else through as text.
//
// Deliberately not a markdown library: the full CommonMark surface is not
// reachable from a chat turn, and a parser that never touches innerHTML cannot
// inject agent output into the DOM as markup.

import { useMemo, type ReactNode } from "react";

type ListItem = { text: string; children: ListItem[] };

type Block =
  | { kind: "code"; lang: string; text: string }
  | { kind: "heading"; text: string }
  | { kind: "list"; ordered: boolean; items: ListItem[] }
  | { kind: "table"; headers: string[]; rows: string[][] }
  | { kind: "quote"; lines: string[] }
  | { kind: "para"; text: string };

const FENCE = /^```(\w*)\s*$/;
const HEADING = /^#{1,6}\s+(.*)$/;
const BULLET = /^(\s*)[-*+]\s+(.*)$/;
const ORDERED = /^(\s*)\d+[.)]\s+(.*)$/;
const BLOCKQUOTE = /^>\s?(.*)$/;
const TABLE_SEP = /^\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?\s*$/;

function parseListItem(line: string): { ordered: boolean; indent: number; text: string } | null {
  const bullet = BULLET.exec(line);
  if (bullet) return { ordered: false, indent: bullet[1].length, text: bullet[2] };
  const ordered = ORDERED.exec(line);
  if (ordered) return { ordered: true, indent: ordered[1].length, text: ordered[2] };
  return null;
}

function addListItemToBlock(
  block: Extract<Block, { kind: "list" }>,
  indent: number,
  text: string,
) {
  const item: ListItem = { text, children: [] };
  if (block.items.length === 0 || indent === 0) {
    block.items.push(item);
    return;
  }
  let parent = block.items[block.items.length - 1];
  let parentIndent = 0;
  while (indent > parentIndent + 1 && parent.children.length > 0) {
    parent = parent.children[parent.children.length - 1];
    parentIndent += 2;
  }
  parent.children.push(item);
}

function parseTable(lines: string[], start: number): { block: Block; next: number } | null {
  const headerLine = lines[start];
  const sepLine = lines[start + 1];
  if (!headerLine?.includes("|") || !sepLine || !TABLE_SEP.test(sepLine)) return null;

  const splitRow = (row: string) =>
    row
      .trim()
      .replace(/^\|/, "")
      .replace(/\|$/, "")
      .split("|")
      .map((cell) => cell.trim());

  const headers = splitRow(headerLine);
  const rows: string[][] = [];
  let i = start + 2;
  while (i < lines.length && lines[i].includes("|") && lines[i].trim()) {
    rows.push(splitRow(lines[i]));
    i += 1;
  }
  return { block: { kind: "table", headers, rows }, next: i };
}

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

    const table = parseTable(lines, i);
    if (table) {
      flush();
      blocks.push(table.block);
      i = table.next - 1;
      continue;
    }

    const heading = HEADING.exec(line);
    if (heading) {
      flush();
      blocks.push({ kind: "heading", text: heading[1] });
      continue;
    }

    const quote = BLOCKQUOTE.exec(line);
    if (quote) {
      flush();
      const quoteLines = [quote[1]];
      i += 1;
      while (i < lines.length) {
        const next = BLOCKQUOTE.exec(lines[i]);
        if (!next) break;
        quoteLines.push(next[1]);
        i += 1;
      }
      blocks.push({ kind: "quote", lines: quoteLines });
      i -= 1;
      continue;
    }

    const listItem = parseListItem(line);
    if (listItem) {
      flush();
      const tail = blocks[blocks.length - 1];
      if (!(tail?.kind === "list" && tail.ordered === listItem.ordered)) {
        blocks.push({ kind: "list", ordered: listItem.ordered, items: [] });
      }
      const listBlock = blocks[blocks.length - 1] as Extract<Block, { kind: "list" }>;
      addListItemToBlock(listBlock, listItem.indent, listItem.text);
      continue;
    }

    if (!line.trim()) flush();
    else paragraph.push(line);
  }
  flush();
  return blocks;
}

const INLINE =
  /(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\((?:https?:\/\/[^)\s]+)\))/g;

function safeLink(href: string, label: string, key: string): ReactNode {
  try {
    const url = new URL(href);
    if (url.protocol !== "http:" && url.protocol !== "https:") return label;
    return (
      <a key={key} className="md-link" href={href} target="_blank" rel="noopener noreferrer">
        {label}
      </a>
    );
  } catch {
    return label;
  }
}

/** Inline code, bold, and http(s) links. Anything else stays literal text. */
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
    const link = /^\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)$/.exec(part);
    if (link) return safeLink(link[2], link[1], key);
    if (part === undefined || part === "") return null;
    return part;
  });
}

function renderListItems(items: ListItem[], ordered: boolean, keyPrefix: string): ReactNode {
  const Tag = ordered ? "ol" : "ul";
  return (
    <Tag className="md-list">
      {items.map((item, index) => {
        const key = `${keyPrefix}-${index}`;
        return (
          <li key={key}>
            {renderInline(item.text, key)}
            {item.children.length > 0 ? renderListItems(item.children, ordered, `${key}-n`) : null}
          </li>
        );
      })}
    </Tag>
  );
}

// ponytail: no reveal throttle. Source changes at most once per completed
// paragraph now, so there is nothing to smooth — the throttle existed for
// token-by-token streaming, which the conversation no longer does.
export default function Markdown({ source }: { source: string }) {
  const blocks = useMemo(() => parseBlocks(source), [source]);
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
          return (
            <div key={key} className="md-list-wrap">
              {renderListItems(block.items, block.ordered, key)}
            </div>
          );
        }
        if (block.kind === "table") {
          return (
            <div key={key} className="md-table-wrap">
              <table className="md-table">
                <thead>
                  <tr>
                    {block.headers.map((cell, cellIndex) => (
                      <th key={`${key}-h-${cellIndex}`}>{renderInline(cell, `${key}-h-${cellIndex}`)}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {block.rows.map((row, rowIndex) => (
                    <tr key={`${key}-r-${rowIndex}`}>
                      {row.map((cell, cellIndex) => (
                        <td key={`${key}-r-${rowIndex}-${cellIndex}`}>
                          {renderInline(cell, `${key}-r-${rowIndex}-${cellIndex}`)}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          );
        }
        if (block.kind === "quote") {
          return (
            <blockquote key={key} className="md-quote">
              {block.lines.map((line, lineIndex) => (
                <p key={`${key}-q-${lineIndex}`} className="md-para">
                  {renderInline(line, `${key}-q-${lineIndex}`)}
                </p>
              ))}
            </blockquote>
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
