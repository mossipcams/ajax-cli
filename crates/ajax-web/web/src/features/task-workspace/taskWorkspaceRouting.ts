import { sessionHash, taskHash } from "@/shared/lib/routes";
import type { BrowserCockpitView, BrowserTaskDetail } from "@/shared/lib/types";
import { readTaskTerminalPreferred } from "./taskViewPreference";

export function cockpitSessionCapable(
  handle: string,
  cockpit: BrowserCockpitView | null | undefined,
): boolean {
  return (
    cockpit?.cards?.some(
      (card) => card.qualified_handle === handle && card.session_capable,
    ) ?? false
  );
}

export function detailSessionCapable(
  detail: BrowserTaskDetail | null | undefined,
  handle: string,
  cockpit: BrowserCockpitView | null | undefined,
): boolean {
  if (!detail || detail.qualified_handle !== handle) return false;
  if (detail.session_capable === false) return false;
  return cockpitSessionCapable(handle, cockpit);
}

export function resolveTaskWorkspaceHash(
  handle: string,
  options: {
    orchestrationChat: boolean;
    sessionCapable: boolean;
    terminalPreferred?: boolean;
  },
): string {
  const terminalPreferred = options.terminalPreferred ?? readTaskTerminalPreferred(handle);
  if (options.orchestrationChat && options.sessionCapable && !terminalPreferred) {
    return sessionHash(handle);
  }
  return taskHash(handle);
}

export function shouldRedirectSessionToTerminal(
  handle: string,
  detail: BrowserTaskDetail | null | undefined,
): boolean {
  if (!detail || detail.qualified_handle !== handle) return false;
  if (detail.session_capable === false) return true;
  return readTaskTerminalPreferred(handle);
}

export function openTaskWorkspaceHash(
  handle: string,
  options: {
    orchestrationChat: boolean;
    sessionCapable: boolean;
  },
): string {
  return resolveTaskWorkspaceHash(handle, options);
}
