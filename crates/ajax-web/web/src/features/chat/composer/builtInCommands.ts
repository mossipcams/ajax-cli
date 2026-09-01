import type { LiveAvailableCommand } from "@/shared/lib/liveSessionCommands";

export const BUILT_IN_CLEAR_COMMAND = "/clear";

export const BUILT_IN_SLASH_COMMANDS: LiveAvailableCommand[] = [
  {
    name: "clear",
    description: "Start a fresh agent context in this chat",
  },
];

export function isBuiltInClearCommand(text: string): boolean {
  return text.trim() === BUILT_IN_CLEAR_COMMAND;
}

/** Ajax-owned slash commands precede harness-advertised ones with the same name. */
export function mergeSlashCommands(
  advertised: LiveAvailableCommand[] | undefined,
): LiveAvailableCommand[] {
  const builtInNames = new Set(BUILT_IN_SLASH_COMMANDS.map((command) => command.name));
  const merged = [...BUILT_IN_SLASH_COMMANDS];
  for (const command of advertised ?? []) {
    if (!builtInNames.has(command.name)) merged.push(command);
  }
  return merged;
}
