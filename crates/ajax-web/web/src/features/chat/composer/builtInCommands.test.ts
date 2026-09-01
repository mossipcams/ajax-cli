import { describe, expect, it } from "vitest";
import {
  BUILT_IN_CLEAR_COMMAND,
  isBuiltInClearCommand,
  mergeSlashCommands,
} from "./builtInCommands";

describe("builtInCommands", () => {
  it("recognizes the clear command exactly", () => {
    expect(isBuiltInClearCommand(BUILT_IN_CLEAR_COMMAND)).toBe(true);
    expect(isBuiltInClearCommand(" /clear ")).toBe(true);
    expect(isBuiltInClearCommand("/clear extra")).toBe(false);
    expect(isBuiltInClearCommand("/help")).toBe(false);
  });

  it("prepends built-in commands ahead of advertised ones", () => {
    expect(
      mergeSlashCommands([{ name: "help", description: "Harness help" }]).map(
        (command) => command.name,
      ),
    ).toEqual(["clear", "help"]);
  });

  it("prefers the built-in clear definition over an advertised duplicate", () => {
    const merged = mergeSlashCommands([
      { name: "clear", description: "Harness clear" },
      { name: "web", description: "Query the web" },
    ]);
    expect(merged).toHaveLength(2);
    expect(merged[0]).toEqual({
      name: "clear",
      description: "Start a fresh agent context in this chat",
    });
  });
});
