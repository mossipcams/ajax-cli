import type { SessionStarterContext } from "./SessionStarter";

/** Treat "within this many px of the bottom" as following the live edge. */
export const PIN_THRESHOLD_PX = 48;
export const RECONNECT_BASE_MS = 500;
export const RECONNECT_MAX_MS = 8000;
/** Handshake failures before the first ready are capped; post-ready drops retry forever. */
export const MAX_HANDSHAKE_ATTEMPTS = 5;

export function formatSessionBrief(context: SessionStarterContext): string {
  const lines = [context.title.trim()];
  if (context.constraints.trim()) lines.push(`\nConstraints: ${context.constraints.trim()}`);
  if (context.expectedOutcome.trim()) lines.push(`\nDone when: ${context.expectedOutcome.trim()}`);
  return lines.join("\n");
}

export function sessionSeededStorageKey(handle: string): string {
  return `ajax.web.session.seeded:${handle}`;
}
