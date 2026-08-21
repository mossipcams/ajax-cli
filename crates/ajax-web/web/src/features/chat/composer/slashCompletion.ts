import type { LiveAvailableCommand } from "@/shared/lib/liveSessionCommands";

export type SlashPrefixMatch = {
  prefix: string;
};

/** First token is `/name` with no whitespace yet (args hide the menu). */
export function parseSlashPrefix(draft: string): SlashPrefixMatch | null {
  if (!draft.startsWith("/")) return null;
  const rest = draft.slice(1);
  if (rest.includes(" ") || rest.includes("\n") || rest.includes("\t")) return null;
  return { prefix: rest.toLowerCase() };
}

export function filterAdvertisedCommands(
  commands: LiveAvailableCommand[] | undefined,
  prefix: string,
): LiveAvailableCommand[] {
  if (!commands?.length) return [];
  const needle = prefix.toLowerCase();
  if (!needle) return commands;
  return commands.filter((command) => command.name.toLowerCase().startsWith(needle));
}

export function insertSlashCommand(command: LiveAvailableCommand): string {
  const suffix = command.inputHint ? " " : "";
  return `/${command.name}${suffix}`;
}

export function slashMenuVisible(
  draft: string,
  commands: LiveAvailableCommand[] | undefined,
): boolean {
  const match = parseSlashPrefix(draft);
  if (!match) return false;
  return filterAdvertisedCommands(commands, match.prefix).length > 0;
}
