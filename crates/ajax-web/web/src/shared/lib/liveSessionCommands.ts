/** One advertised ACP slash command from protocol v2 `availableCommands`. */
export interface LiveAvailableCommand {
  name: string;
  description: string;
  inputHint?: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object";
}

export function parseLiveAvailableCommands(raw: unknown): LiveAvailableCommand[] | undefined {
  if (!Array.isArray(raw)) return undefined;
  const parsed: LiveAvailableCommand[] = [];
  for (const item of raw) {
    if (!isRecord(item) || typeof item.name !== "string" || typeof item.description !== "string") {
      return undefined;
    }
    parsed.push({
      name: item.name,
      description: item.description,
      ...(typeof item.inputHint === "string" && item.inputHint.trim()
        ? { inputHint: item.inputHint }
        : {}),
    });
  }
  return parsed.length ? parsed : emptyAdvertisedList(raw);
}

function emptyAdvertisedList(raw: unknown[]): LiveAvailableCommand[] | undefined {
  return raw.length === 0 ? [] : undefined;
}
