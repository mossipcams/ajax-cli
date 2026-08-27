import { sessionHash, taskHash } from "@/shared/lib/routes";
import type { BrowserCockpitView, BrowserTaskDetail } from "@/shared/lib/types";
import { readTaskTerminalPreferred } from "./taskViewPreference";

/** Mirrors host `supports_acp_session` / `acp_launch_for_agent` allowlist. */
export function isAcpCapableAgent(agent: string | null | undefined): boolean {
  if (!agent?.trim()) return false;
  switch (agent.trim().toLowerCase()) {
    case "codex":
    case "claude":
    case "cursor":
    case "pi":
      return true;
    default:
      return false;
  }
}

export function taskOffersOrchestrationChat(
  detail: Pick<BrowserTaskDetail, "session_capable" | "agent">,
): boolean {
  if (detail.session_capable !== false) return true;
  return isAcpCapableAgent(detail.agent);
}

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
  if (detail.session_capable === false && !isAcpCapableAgent(detail.agent)) {
    return true;
  }
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
