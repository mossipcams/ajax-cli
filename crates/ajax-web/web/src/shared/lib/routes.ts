// Hash-route parsing and formatting. Pure functions extracted from the legacy
// `applyRoute` so route selection stays framework-agnostic and is not
// re-derived by whatever renders the route.

import type { RouteKind } from "./types";

export interface Route {
  kind: RouteKind;
  project?: string;
  handle?: string;
  pr?: number;
}

const TASK_PREFIX = "#/t/";
const PROJECT_PREFIX = "#/p/";
const SESSION_PREFIX = "#/session/";
const DIFF_SUFFIX = "/diff";

function safeDecode(s: string): string {
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
}

function isBlankDecoded(s: string): boolean {
  return !s || s.trim() === "" || s === "/";
}

function parsePrQuery(query: string): number | undefined {
  if (!query) return undefined;
  for (const part of query.split("&")) {
    const [key, value] = part.split("=");
    if (key === "pr" && value && /^\d+$/.test(value)) {
      const n = Number(value);
      if (Number.isSafeInteger(n) && n > 0) return n;
    }
  }
  return undefined;
}

export function parseRoute(hash: string): Route {
  const raw = hash || "#/";
  const qIndex = raw.indexOf("?");
  const value = qIndex >= 0 ? raw.slice(0, qIndex) : raw;
  const query = qIndex >= 0 ? raw.slice(qIndex + 1) : "";

  if (value === "#/settings" || value === "#/control") return { kind: "settings" };
  if (value === "#/session") return { kind: "session" };
  if (value.startsWith(SESSION_PREFIX)) {
    const handle = safeDecode(value.slice(SESSION_PREFIX.length));
    if (!handle) return { kind: "session" };
    return { kind: "session", handle };
  }
  if (value.startsWith(TASK_PREFIX)) {
    const rest = value.slice(TASK_PREFIX.length).replace(/\/diff\/$/, DIFF_SUFFIX);
    if (!rest) return { kind: "dashboard" };
    const segments = rest.split("/");
    if (segments.length === 2 && segments[1] === "diff") {
      const encodedHandle = segments[0];
      if (!encodedHandle) return { kind: "dashboard" };
      const handle = safeDecode(encodedHandle);
      if (isBlankDecoded(handle)) return { kind: "dashboard" };
      const pr = parsePrQuery(query);
      return pr !== undefined ? { kind: "diff", handle, pr } : { kind: "diff", handle };
    }
    if (segments.length === 1) {
      const handle = safeDecode(segments[0]);
      if (isBlankDecoded(handle)) return { kind: "dashboard" };
      return { kind: "task", handle };
    }
    return { kind: "dashboard" };
  }
  if (value.startsWith(PROJECT_PREFIX)) {
    const project = safeDecode(value.slice(PROJECT_PREFIX.length));
    if (isBlankDecoded(project)) return { kind: "dashboard" };
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

/** @deprecated Legacy `#/control` hash; use {@link settingsHash}. */
export function controlHash(): string {
  return settingsHash();
}

export function projectHash(project: string): string {
  return `${PROJECT_PREFIX}${encodeURIComponent(project)}`;
}

export function taskHash(handle: string): string {
  return `${TASK_PREFIX}${encodeURIComponent(handle)}`;
}

export function taskDiffHash(handle: string, pr?: number): string {
  const base = `${TASK_PREFIX}${encodeURIComponent(handle)}${DIFF_SUFFIX}`;
  return pr !== undefined ? `${base}?pr=${pr}` : base;
}

export function sessionHash(handle?: string): string {
  return handle ? `${SESSION_PREFIX}${encodeURIComponent(handle)}` : "#/session";
}
