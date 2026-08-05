import type { ReactNode } from "react";
import type { WebSessionSymbolContext } from "./types";

export interface KnownSymbolIndex {
  byExactName: Map<string, WebSessionSymbolContext[]>;
}

export function buildKnownSymbolIndex(
  symbols: Iterable<WebSessionSymbolContext>,
): KnownSymbolIndex {
  const byExactName = new Map<string, WebSessionSymbolContext[]>();
  for (const symbol of symbols) {
    const list = byExactName.get(symbol.name) ?? [];
    if (!list.some((item) => item.id === symbol.id)) {
      list.push(symbol);
      byExactName.set(symbol.name, list);
    }
  }
  return { byExactName };
}

export function mergeKnownSymbols(
  current: WebSessionSymbolContext[],
  incoming: Iterable<WebSessionSymbolContext>,
): WebSessionSymbolContext[] {
  const byId = new Map(current.map((symbol) => [symbol.id, symbol]));
  for (const symbol of incoming) {
    byId.set(symbol.id, symbol);
  }
  return Array.from(byId.values());
}

export function resolveKnownSymbol(
  index: KnownSymbolIndex,
  token: string,
): WebSessionSymbolContext | null {
  const matches = index.byExactName.get(token);
  if (!matches || matches.length !== 1) return null;
  return matches[0] ?? null;
}

interface TextSpan {
  start: number;
  end: number;
  symbol: WebSessionSymbolContext;
  label: string;
}

const BACKTICK_PATTERN = /`([^`]+)`/g;
const PLAIN_TOKEN_PATTERN =
  /\b([A-Z][a-zA-Z0-9_]*(?:\.[a-z_][a-zA-Z0-9_]*)?|[a-z][a-z0-9_]*(?:\.[a-z_][a-z0-9_]*)*)\b/g;

function overlaps(span: TextSpan, start: number, end: number): boolean {
  return span.start < end && span.end > start;
}

function collectSymbolSpans(text: string, index: KnownSymbolIndex): TextSpan[] {
  const spans: TextSpan[] = [];

  for (const match of text.matchAll(BACKTICK_PATTERN)) {
    const token = match[1];
    const start = match.index ?? 0;
    const end = start + match[0].length;
    if (!token) continue;
    const symbol = resolveKnownSymbol(index, token);
    if (!symbol) continue;
    spans.push({ start, end, symbol, label: token });
  }

  for (const match of text.matchAll(PLAIN_TOKEN_PATTERN)) {
    const token = match[1];
    const start = match.index ?? 0;
    const end = start + match[0].length;
    if (!token) continue;
    if (spans.some((span) => overlaps(span, start, end))) continue;
    const symbol = resolveKnownSymbol(index, token);
    if (!symbol) continue;
    spans.push({ start, end, symbol, label: token });
  }

  return spans.sort((left, right) => left.start - right.start);
}

export function renderMessageContent(
  text: string,
  index: KnownSymbolIndex,
  onSymbolClick: (symbol: WebSessionSymbolContext) => void,
): ReactNode {
  const spans = collectSymbolSpans(text, index);
  if (spans.length === 0) return text;

  const nodes: ReactNode[] = [];
  let cursor = 0;

  for (const span of spans) {
    if (span.start > cursor) {
      nodes.push(text.slice(cursor, span.start));
    }
    nodes.push(
      <button
        key={`${span.symbol.id}-${span.start}`}
        type="button"
        className="ajax-web-session-symbol-ref"
        data-testid={`ajax-web-session-symbol-ref-${span.symbol.id}`}
        onClick={() => onSymbolClick(span.symbol)}
      >
        {span.label}
      </button>,
    );
    cursor = span.end;
  }

  if (cursor < text.length) {
    nodes.push(text.slice(cursor));
  }

  return nodes;
}
