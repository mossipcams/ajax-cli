import type { Terminal } from "@xterm/xterm";

/** HTTP(S) only — shared by hit-test and WebLinksAddon. */
export const HTTP_URL_REGEX = /(https?:\/\/[^\s"'<>]+)/i;

/** Matches xterm WebLinksAddon trailing-punctuation trim behavior. */
const TRAILING_URL_PUNCTUATION = /[.,;:!?)}\]'"]+$/;

export type HttpLinkHit = { url: string };

export function findHttpLinkAtClient(
  term: Terminal | undefined,
  clientX: number,
  clientY: number,
  hostEl?: HTMLElement | null,
): HttpLinkHit | null {
  if (!term?.element || term.cols <= 0 || term.rows <= 0) return null;

  const screenEl = term.element.querySelector<HTMLElement>(".xterm-screen");
  const bounds = screenEl?.getBoundingClientRect() ?? hostEl?.getBoundingClientRect();
  if (!bounds || bounds.width <= 0 || bounds.height <= 0) return null;

  const relX = clientX - bounds.left;
  const relY = clientY - bounds.top;
  if (relX < 0 || relY < 0 || relX > bounds.width || relY > bounds.height) return null;

  const cellWidth = bounds.width / term.cols;
  const cellHeight = bounds.height / term.rows;
  const col = Math.min(term.cols - 1, Math.max(0, Math.floor(relX / cellWidth)));
  const rowInView = Math.min(term.rows - 1, Math.max(0, Math.floor(relY / cellHeight)));
  const bufferRow = term.buffer.active.viewportY + rowInView;
  const line = term.buffer.active.getLine(bufferRow);
  if (!line) return null;

  const lineStr = line.translateToString(false);
  const regex = new RegExp(HTTP_URL_REGEX.source, `${HTTP_URL_REGEX.flags}g`);
  let match: RegExpExecArray | null;
  while ((match = regex.exec(lineStr)) !== null) {
    const raw = match[0];
    const start = match.index;
    const end = start + raw.length - 1;
    if (col < start || col > end) continue;

    const url = raw.replace(TRAILING_URL_PUNCTUATION, "");
    try {
      const parsed = new URL(url);
      if (parsed.protocol !== "http:" && parsed.protocol !== "https:") continue;
      return { url };
    } catch {
      continue;
    }
  }

  return null;
}
