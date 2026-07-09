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
});
