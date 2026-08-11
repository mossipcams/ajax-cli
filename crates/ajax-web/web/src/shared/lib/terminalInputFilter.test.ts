import { describe, expect, it } from "vitest";
import { filterTerminalInputReports } from "./terminalInputFilter";

const ESC = "\u001b";

describe("filterTerminalInputReports", () => {
  it("strips a flood of device-attribute replies to empty", () => {
    const flood =
      `${ESC}[>84;0;0c` +
      `${ESC}[>84;0;0c` +
      `${ESC}[?1;2c` +
      `${ESC}[>0;276;0c` +
      `${ESC}[?1;2c` +
      `${ESC}[>84;0;0c`;
    expect(filterTerminalInputReports(flood)).toBe("");
  });

  it("preserves printable text and common control keys", () => {
    expect(filterTerminalInputReports("hello")).toBe("hello");
    expect(filterTerminalInputReports("\r")).toBe("\r");
    expect(filterTerminalInputReports("\u007f")).toBe("\u007f");
    expect(filterTerminalInputReports("\u0003")).toBe("\u0003");
    expect(filterTerminalInputReports(`${ESC}[A`)).toBe(`${ESC}[A`);
    expect(filterTerminalInputReports(`${ESC}[1;5D`)).toBe(`${ESC}[1;5D`);
  });

  it("strips embedded DA replies while keeping surrounding user text", () => {
    expect(filterTerminalInputReports(`hello${ESC}[?1;2cworld`)).toBe("helloworld");
    expect(filterTerminalInputReports(`a${ESC}[>84;0;0cb`)).toBe("ab");
  });

  it("preserves bracketed paste envelopes and payload", () => {
    const bracketed = `${ESC}[200~paste content${ESC}[201~`;
    expect(filterTerminalInputReports(bracketed)).toBe(bracketed);
  });

  it("leaves lone ESC, incomplete CSI, and F3/CPR-shaped R sequences untouched", () => {
    expect(filterTerminalInputReports(ESC)).toBe(ESC);
    expect(filterTerminalInputReports(`${ESC}[`)).toBe(`${ESC}[`);
    expect(filterTerminalInputReports(`${ESC}[?`)).toBe(`${ESC}[?`);
    // Modified F3 — must not be treated as a cursor-position report.
    expect(filterTerminalInputReports(`${ESC}[1;5R`)).toBe(`${ESC}[1;5R`);
    expect(filterTerminalInputReports(`${ESC}[24;80R`)).toBe(`${ESC}[24;80R`);
  });
});
