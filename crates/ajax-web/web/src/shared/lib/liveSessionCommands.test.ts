import { describe, expect, it } from "vitest";
import { parseLiveAvailableCommands } from "./liveSessionCommands";

describe("parseLiveAvailableCommands", () => {
  it("parses advertised commands from snapshot payloads", () => {
    expect(
      parseLiveAvailableCommands([
        { name: "web", description: "Query the web", inputHint: "query" },
        { name: "help", description: "Show help" },
      ]),
    ).toEqual([
      { name: "web", description: "Query the web", inputHint: "query" },
      { name: "help", description: "Show help" },
    ]);
  });

  it("accepts an explicit empty replacement list", () => {
    expect(parseLiveAvailableCommands([])).toEqual([]);
  });

  it("rejects malformed entries", () => {
    expect(parseLiveAvailableCommands([{ name: "web" }])).toBeUndefined();
  });
});
