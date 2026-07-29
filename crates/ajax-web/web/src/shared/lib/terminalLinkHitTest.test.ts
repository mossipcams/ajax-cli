import { describe, it, expect } from "vitest";
import type { Terminal } from "@xterm/xterm";
import { findHttpLinkAtClient } from "./terminalLinkHitTest";

function createMockTerminal(options: {
  cols?: number;
  rows?: number;
  viewportY?: number;
  lineText: string;
  screenRect?: DOMRect;
  hostRect?: DOMRect;
}): { term: Terminal; hostEl: HTMLElement } {
  const cols = options.cols ?? 80;
  const rows = options.rows ?? 24;
  const viewportY = options.viewportY ?? 0;
  const screenRect =
    options.screenRect ??
    new DOMRect(100, 50, cols * 10, rows * 20);
  const hostRect = options.hostRect ?? screenRect;

  const screenEl = document.createElement("div");
  screenEl.className = "xterm-screen";
  screenEl.getBoundingClientRect = () => screenRect;

  const termEl = document.createElement("div");
  termEl.className = "xterm";
  termEl.appendChild(screenEl);

  const hostEl = document.createElement("div");
  hostEl.getBoundingClientRect = () => hostRect;

  const term = {
    cols,
    rows,
    element: termEl,
    buffer: {
      active: {
        viewportY,
        getLine: (row: number) => {
          if (row !== viewportY) return undefined;
          return {
            translateToString: () => options.lineText,
          };
        },
      },
    },
  } as unknown as Terminal;

  return { term, hostEl };
}

describe("findHttpLinkAtClient", () => {
  it("returns the URL when click coords are over an http(s) link", () => {
    const lineText = "hello https://example.com/path world";
    const { term, hostEl } = createMockTerminal({ lineText });
    const urlStart = lineText.indexOf("https://");
    const col = urlStart + 10;
    const clientX = 100 + (col + 0.5) * (800 / 80);
    const clientY = 50 + 0.5 * (480 / 24);

    expect(findHttpLinkAtClient(term, clientX, clientY, hostEl)).toEqual({
      url: "https://example.com/path",
    });
  });

  it("returns null when click coords are over non-URL text", () => {
    const lineText = "hello https://example.com/path world";
    const { term, hostEl } = createMockTerminal({ lineText });
    const helloCol = 2;
    const clientX = 100 + (helloCol + 0.5) * (800 / 80);
    const clientY = 50 + 0.5 * (480 / 24);

    expect(findHttpLinkAtClient(term, clientX, clientY, hostEl)).toBeNull();
  });

  it("rejects non-http(s) schemes even if matched by regex", () => {
    const lineText = "see ftp://example.com/path here";
    const { term, hostEl } = createMockTerminal({ lineText });
    const start = lineText.indexOf("ftp://");
    const col = start + 2;
    const clientX = 100 + (col + 0.5) * (800 / 80);
    const clientY = 50 + 0.5 * (480 / 24);

    expect(findHttpLinkAtClient(term, clientX, clientY, hostEl)).toBeNull();
  });

  it("trims common trailing punctuation from the matched URL", () => {
    const lineText = "visit https://example.com/path).";
    const { term, hostEl } = createMockTerminal({ lineText });
    const urlStart = lineText.indexOf("https://");
    const col = urlStart + 20;
    const clientX = 100 + (col + 0.5) * (800 / 80);
    const clientY = 50 + 0.5 * (480 / 24);

    expect(findHttpLinkAtClient(term, clientX, clientY, hostEl)).toEqual({
      url: "https://example.com/path",
    });
  });
});
