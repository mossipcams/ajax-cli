/** Agent-reported session title from protocol v2 `sessionTitle`. */
export function parseLiveSessionTitle(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}
