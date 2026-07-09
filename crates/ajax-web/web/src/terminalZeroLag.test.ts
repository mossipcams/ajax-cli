import { describe, it, expect, vi } from "vitest";
import {
  createZeroLagEcho,
  zeroLagOverlayStyle,
  type ZeroLagCursorMetrics,
} from "./terminalZeroLag";

const metrics = (partial: Partial<ZeroLagCursorMetrics> = {}): ZeroLagCursorMetrics => ({
  cursorX: 3,
  cursorY: 2,
  cols: 80,
  rows: 24,
  canvasWidth: 800,
  canvasHeight: 480,
  fontSize: 13,
  ...partial,
});

describe("createZeroLagEcho", () => {
  it("append then text() returns concatenated printable string", () => {
    const onChange = vi.fn();
    const echo = createZeroLagEcho({
      onChange,
      measure: () => metrics(),
    });

    echo.noteBeforeInputPrintable("h");
    echo.noteBeforeInputPrintable("i");

    expect(echo.text()).toBe("hi");
    expect(onChange).toHaveBeenLastCalledWith("hi", expect.stringContaining("left:"));
  });

  it("noteBeforeInputPrintable then matching onTerminalData does not double-append", () => {
    const echo = createZeroLagEcho({
      onChange: vi.fn(),
      measure: () => metrics(),
    });

    echo.noteBeforeInputPrintable("a");
    echo.onTerminalData("a");

    expect(echo.text()).toBe("a");
  });

  it("onTerminalData without ahead appends printable", () => {
    const echo = createZeroLagEcho({
      onChange: vi.fn(),
      measure: () => metrics(),
    });

    echo.onTerminalData("b");

    expect(echo.text()).toBe("b");
  });

  it("trim removes one character; beforeinput backspace then \\x7f does not double-trim", () => {
    const echo = createZeroLagEcho({
      onChange: vi.fn(),
      measure: () => metrics(),
    });

    echo.noteBeforeInputPrintable("hi");
    echo.noteBeforeInputBackspace();
    expect(echo.text()).toBe("h");

    // Ghostty will also emit \\x7f; ahead counter must absorb it without a second trim.
    echo.noteBeforeInputPrintable("i");
    echo.noteBeforeInputBackspace();
    expect(echo.text()).toBe("h");
    echo.onTerminalData("\x7f");
    expect(echo.text()).toBe("h");
  });

  it("clear and Enter path empty text", () => {
    const onChange = vi.fn();
    const echo = createZeroLagEcho({
      onChange,
      measure: () => metrics(),
    });

    echo.noteBeforeInputPrintable("hi");
    echo.clear();
    expect(echo.text()).toBe("");
    expect(onChange).toHaveBeenLastCalledWith("", "");

    echo.noteBeforeInputPrintable("x");
    echo.onTerminalData("\r");
    expect(echo.text()).toBe("");
  });

  it("clearIfEchoedIn clears when pending is a substring and leaves unrelated chunks", () => {
    const echo = createZeroLagEcho({
      onChange: vi.fn(),
      measure: () => metrics(),
    });

    echo.noteBeforeInputPrintable("hello");
    echo.clearIfEchoedIn("status bar");
    expect(echo.text()).toBe("hello");

    echo.clearIfEchoedIn("say hello now");
    expect(echo.text()).toBe("");
  });

  it("clearIfEchoedIn consumes matching prefixes from sequential chunks", () => {
    const echo = createZeroLagEcho({
      onChange: vi.fn(),
      measure: () => metrics(),
    });

    echo.noteBeforeInputPrintable("hi");
    echo.clearIfEchoedIn("h");
    expect(echo.text()).toBe("i");
    echo.clearIfEchoedIn("i");
    expect(echo.text()).toBe("");
  });

  it("clearIfEchoedIn ignores unrelated chunks that are not a pending prefix", () => {
    const echo = createZeroLagEcho({
      onChange: vi.fn(),
      measure: () => metrics(),
    });

    echo.noteBeforeInputPrintable("hi");
    echo.clearIfEchoedIn("status bar redraw");
    expect(echo.text()).toBe("hi");
  });

  it("clearIfEchoedIn clears when pending appears as full substring in chunk", () => {
    const echo = createZeroLagEcho({
      onChange: vi.fn(),
      measure: () => metrics(),
    });

    echo.noteBeforeInputPrintable("hi");
    echo.clearIfEchoedIn("prefix hi suffix");
    expect(echo.text()).toBe("");
  });
});

describe("zeroLagOverlayStyle", () => {
  it("returns left/top/font-size/line-height without bottom", () => {
    const style = zeroLagOverlayStyle(metrics());
    expect(style).toContain("left:");
    expect(style).toContain("top:");
    expect(style).toContain("font-size:");
    expect(style).toContain("line-height:");
    expect(style).not.toContain("bottom:");
  });

  it("prefers explicit cell metrics over canvas divided by grid", () => {
    const style = zeroLagOverlayStyle(
      metrics({
        cursorX: 2,
        cursorY: 3,
        cellWidth: 10,
        cellHeight: 20,
        canvasWidth: 800,
        canvasHeight: 480,
        cols: 80,
        rows: 24,
      }),
    );
    expect(style).toContain("left: 20px");
    expect(style).toContain("top: 60px");
    expect(style).toContain("line-height: 20px");

    const style2 = zeroLagOverlayStyle(
      metrics({
        cursorX: 1,
        cursorY: 4,
        cellWidth: 8,
        cellHeight: 16,
        canvasWidth: 800,
        canvasHeight: 800,
        cols: 80,
        rows: 24,
      }),
    );
    expect(style2).toContain("left: 8px");
    expect(style2).toContain("top: 64px");
  });

  it("falls back to canvas divided by grid when cell metrics are missing", () => {
    const style = zeroLagOverlayStyle(
      metrics({
        cursorX: 2,
        cursorY: 3,
        canvasWidth: 800,
        canvasHeight: 480,
        cols: 80,
        rows: 24,
      }),
    );
    expect(style).toContain("left: 20px");
    expect(style).toContain("top: 60px");
    expect(style).toContain("line-height: 20px");
  });
});
