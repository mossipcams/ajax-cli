import { describe, expect, it } from "vitest";
import {
  filterAdvertisedCommands,
  insertSlashCommand,
  parseSlashPrefix,
  slashMenuVisible,
} from "./slashCompletion";
import type { LiveAvailableCommand } from "@/shared/lib/liveSessionCommands";

const commands: LiveAvailableCommand[] = [
  { name: "web", description: "Query the web", inputHint: "query" },
  { name: "help", description: "Show help" },
];

describe("slashCompletion", () => {
  it("parses a slash prefix before whitespace", () => {
    expect(parseSlashPrefix("/we")).toEqual({ prefix: "we" });
    expect(parseSlashPrefix("/web query")).toBeNull();
    expect(parseSlashPrefix("plain")).toBeNull();
  });

  it("filters advertised commands case-insensitively", () => {
    expect(filterAdvertisedCommands(commands, "w")).toEqual([commands[0]]);
    expect(filterAdvertisedCommands(commands, "WEB")).toEqual([commands[0]]);
    expect(filterAdvertisedCommands(commands, "foo")).toEqual([]);
  });

  it("inserts a trailing space when inputHint is present", () => {
    expect(insertSlashCommand(commands[0]!)).toBe("/web ");
    expect(insertSlashCommand(commands[1]!)).toBe("/help");
  });

  it("hides the menu after args or when no matches", () => {
    expect(slashMenuVisible("/we", commands)).toBe(true);
    expect(slashMenuVisible("/web query", commands)).toBe(false);
    expect(slashMenuVisible("/foo", commands)).toBe(false);
  });
});
