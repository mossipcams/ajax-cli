import { describe, expect, it } from "vitest";
import { filterTerminalInputReports } from "./terminalInputFilter";

describe("filterTerminalInputReports", () => {
  it("strips a flood of device-attribute replies to empty", () => {
    const flood =
      "\x1b[>84;0;0c" +
      "\x1b[>84;0;0c" +
      "\x1b[?1;2c" +
      "\x1b[>0;276;0c" +
      "\x1b[?1;2c" +
      "\x1b[>84;0;0c";
    expect(filterTerminalInputReports(flood)).toBe("");
  });

  it("preserves printable text and common control keys", () => {
    expect(filterTerminalInputReports("hello")).toBe("hello");
    expect(filterTerminalInputReports("\r")).toBe("\r");
    expect(filterTerminalInputReports("\x7f")).toBe("\x7f");
    expect(filterTerminalInputReports("\x03")).toBe("\x03");
    expect(filterTerminalInputReports("\x1b[A")).toBe("\x1b[A");
    expect(filterTerminalInputReports("\x1b[1;5D")).toBe("\x1b[1;5D");
  });

  it("strips embedded DA replies while keeping surrounding user text", () => {
    expect(filterTerminalInputReports("hello\x1b[?1;2cworld")).toBe("helloworld");
    expect(filterTerminalInputReports("a\x1b[>84;0;0cb")).toBe("ab");
  });

  it("preserves bracketed paste envelopes and payload", () => {
    const bracketed = "\x1b[200~paste content\x1b[201~";
    expect(filterTerminalInputReports(bracketed)).toBe(bracketed);
  });

  it("leaves lone ESC, incomplete CSI, and F3/CPR-shaped R sequences untouched", () => {
    expect(filterTerminalInputReports("\x1b")).toBe("\x1b");
    expect(filterTerminalInputReports("\x1b[")).toBe("\x1b[");
    expect(filterTerminalInputReports("\x1b[?")).toBe("\x1b[?");
    // Modified F3 — must not be treated as a cursor-position report.
    expect(filterTerminalInputReports("\x1b[1;5R")).toBe("\x1b[1;5R");
    expect(filterTerminalInputReports("\x1b[24;80R")).toBe("\x1b[24;80R");
  });
});
