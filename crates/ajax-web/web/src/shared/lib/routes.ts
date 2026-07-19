// Hash-route parsing and formatting. Pure functions extracted from the legacy
// `applyRoute` so route selection stays framework-agnostic and is not
// re-derived by whatever renders the route.

import type { RouteKind } from "./types";

export interface Route {
  kind: RouteKind;
  project?: string;
  handle?: string;
}

const TASK_PREFIX = "#/t/";
const PROJECT_PREFIX = "#/p/";

function safeDecode(s: string): string {
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
}

export function parseRoute(hash: string): Route {
  const value = hash || "#/";
  if (value === "#/settings") return { kind: "settings" };
  if (value.startsWith(TASK_PREFIX)) {
    const handle = safeDecode(value.slice(TASK_PREFIX.length));
    if (!handle) return { kind: "dashboard" };
    return { kind: "task", handle };
  }
  if (value.startsWith(PROJECT_PREFIX)) {
    const project = safeDecode(value.slice(PROJECT_PREFIX.length));
    if (!project) return { kind: "dashboard" };
    return { kind: "project", project };
  }
  return { kind: "dashboard" };
}

export function dashboardHash(): string {
  return "#/";
}

export function settingsHash(): string {
  return "#/settings";
}

export function projectHash(project: string): string {
  return `${PROJECT_PREFIX}${encodeURIComponent(project)}`;
}

export function taskHash(handle: string): string {
  return `${TASK_PREFIX}${encodeURIComponent(handle)}`;
}
