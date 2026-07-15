/**
 * Built-in @wterm/core WasmBridge contracts.
 *
 * Surface V2 selected this core in Task 1; this file proves the real WASM
 * scrollback, mode flags, alternate-screen restore, and UTF-8 viewport cells
 * through public APIs only.
 */
import { describe, it, expect } from "vitest";
import { WasmBridge } from "@wterm/core";

const COLS = 80;
const ROWS = 24;
const LINE_COUNT = 100;

function readViewportRow(core: WasmBridge, row: number, maxCols = COLS): string {
  let text = "";
  for (let col = 0; col < maxCols; col += 1) {
    const code = core.getCell(row, col).char;
    if (code === 0) break;
    text += String.fromCodePoint(code);
  }
  return text;
}

function viewportRowGlyphCodes(core: WasmBridge, row: number, maxCols = COLS): number[] {
  const codes: number[] = [];
  for (let col = 0; col < maxCols; col += 1) {
    const code = core.getCell(row, col).char;
    if (code !== 0) codes.push(code);
  }
  return codes;
}

function readScrollbackLine(core: WasmBridge, offset: number): string {
  const len = core.getScrollbackLineLen(offset);
  let text = "";
  for (let col = 0; col < len; col += 1) {
    const code = core.getScrollbackCell(offset, col).char;
    if (code === 0) break;
    text += String.fromCodePoint(code);
  }
  return text.trimEnd();
}

function lineNumberFromMarker(line: string): number {
  const match = /wterm-hist-(\d+)/.exec(line);
  expect(match).not.toBeNull();
  return Number(match![1]);
}

describe("terminalWtermCore integration (built-in WasmBridge)", () => {
  it("returns ordered numbered scrollback cells oldest-to-newest", async () => {
    const core = await WasmBridge.load();
    core.init(COLS, ROWS);

    for (let i = 0; i < LINE_COUNT; i += 1) {
      core.writeString(`wterm-hist-${String(i).padStart(3, "0")}\r\n`);
    }

    const count = core.getScrollbackCount();
    expect(count).toBeGreaterThan(0);
    expect(count).toBeGreaterThanOrEqual(LINE_COUNT - ROWS);

    const lines: string[] = [];
    for (let offset = count - 1; offset >= 0; offset -= 1) {
      const line = readScrollbackLine(core, offset);
      expect(line.length).toBeGreaterThan(0);
      lines.push(line);
    }

    expect(lines.length).toBe(count);

    const numbers = lines.map(lineNumberFromMarker);
    for (let i = 1; i < numbers.length; i += 1) {
      expect(numbers[i]).toBeGreaterThan(numbers[i - 1]);
    }

    const early = lines[0];
    const late = lines[lines.length - 1];
    expect(early).toMatch(/wterm-hist-\d+/);
    expect(late).toMatch(/wterm-hist-\d+/);
    expect(lineNumberFromMarker(early)).toBeLessThan(lineNumberFromMarker(late));
  });

  it("toggles application cursor keys via DECSET/DECRST 1", async () => {
    const core = await WasmBridge.load();
    core.init(COLS, ROWS);

    expect(core.cursorKeysApp()).toBe(false);

    core.writeString("\x1b[?1h");
    expect(core.cursorKeysApp()).toBe(true);

    core.writeString("\x1b[?1l");
    expect(core.cursorKeysApp()).toBe(false);
  });

  it("toggles bracketed paste mode via DECSET/DECRST 2004", async () => {
    const core = await WasmBridge.load();
    core.init(COLS, ROWS);

    expect(core.bracketedPaste()).toBe(false);

    core.writeString("\x1b[?2004h");
    expect(core.bracketedPaste()).toBe(true);

    core.writeString("\x1b[?2004l");
    expect(core.bracketedPaste()).toBe(false);
  });

  it("hides primary viewport text in alternate screen and restores it after DECRST 1049", async () => {
    const core = await WasmBridge.load();
    core.init(COLS, ROWS);

    core.writeString("primary-content\r\n");
    expect(readViewportRow(core, 0)).toContain("primary-content");
    expect(core.usingAltScreen()).toBe(false);

    core.writeString("\x1b[?1049h\x1b[2J\x1b[HALT-SCREEN");
    expect(core.usingAltScreen()).toBe(true);
    expect(readViewportRow(core, 0)).toContain("ALT-SCREEN");
    expect(readViewportRow(core, 0)).not.toContain("primary-content");

    core.writeString("\x1b[?1049l");
    expect(core.usingAltScreen()).toBe(false);
    expect(readViewportRow(core, 0)).toContain("primary-content");
    expect(readViewportRow(core, 0)).not.toContain("ALT-SCREEN");
  });

  it("renders multibyte UTF-8 output in public viewport cells", async () => {
    const core = await WasmBridge.load();
    core.init(COLS, ROWS);

    core.writeString("héllo 世界\r\n");
    const glyphs = viewportRowGlyphCodes(core, 0);

    expect(glyphs).toContain("h".codePointAt(0)!);
    expect(glyphs).toContain("é".codePointAt(0)!);
    expect(glyphs).toContain("世".codePointAt(0)!);
    expect(glyphs).toContain("界".codePointAt(0)!);
  });
});
