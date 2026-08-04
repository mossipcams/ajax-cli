import { parseRoute } from "@/shared/lib/routes";

const INSTALL_ID_KEY = "ajax:telemetry:install_id";
const SESSION_ID_KEY = "ajax:telemetry:session_id";
const SEQUENCE_KEY = "ajax:telemetry:sequence";

function safeStorageGet(storage: Storage, key: string): string | null {
  try {
    return storage.getItem(key);
  } catch {
    return null;
  }
}

function safeStorageSet(storage: Storage, key: string, value: string): void {
  try {
    storage.setItem(key, value);
  } catch {
    // ponytail: storage may be unavailable in private mode; ids stay in-memory for the call.
  }
}

/** Opaque id for install/session/event correlation (not a secret). */
export function generateId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  // Secure fallback when randomUUID is unavailable (non-secure contexts).
  if (typeof crypto !== "undefined" && typeof crypto.getRandomValues === "function") {
    const bytes = new Uint8Array(16);
    crypto.getRandomValues(bytes);
    return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  }
  // Last resort: timestamp only — never Math.random() (CodeQL js/insecure-randomness).
  return `ajax-${Date.now().toString(36)}`;
}

/** True when running as an installed PWA (Home Screen) rather than a browser tab. */
export function isStandaloneDisplay(): boolean {
  if (typeof window === "undefined") {
    return false;
  }
  const nav = navigator as Navigator & { standalone?: boolean };
  if (nav.standalone === true) {
    return true;
  }
  try {
    return window.matchMedia("(display-mode: standalone)").matches;
  } catch {
    return false;
  }
}

export function getInstallId(): string {
  const existing = safeStorageGet(localStorage, INSTALL_ID_KEY);
  if (existing) {
    return existing;
  }
  const id = generateId();
  safeStorageSet(localStorage, INSTALL_ID_KEY, id);
  return id;
}

/** Tab/session scoped — sessionStorage so a new browser session gets a new id. */
export function getSessionId(): string {
  const existing = safeStorageGet(sessionStorage, SESSION_ID_KEY);
  if (existing) {
    return existing;
  }
  const id = generateId();
  safeStorageSet(sessionStorage, SESSION_ID_KEY, id);
  return id;
}

export function nextSequence(): number {
  const installId = getInstallId();
  const raw = safeStorageGet(localStorage, SEQUENCE_KEY);
  let parsed: { install_id: string; seq: number } | null = null;
  if (raw) {
    try {
      parsed = JSON.parse(raw) as { install_id: string; seq: number };
    } catch {
      parsed = null;
    }
  }
  const next =
    parsed && parsed.install_id === installId && Number.isFinite(parsed.seq)
      ? parsed.seq + 1
      : 1;
  safeStorageSet(
    localStorage,
    SEQUENCE_KEY,
    JSON.stringify({ install_id: installId, seq: next }),
  );
  return next;
}

export function readViewport(): { viewport_w: number; viewport_h: number } {
  if (typeof window === "undefined") {
    return { viewport_w: 0, viewport_h: 0 };
  }
  return {
    viewport_w: window.innerWidth,
    viewport_h: window.innerHeight,
  };
}

export function readIosVersion(): string | undefined {
  if (typeof navigator === "undefined") {
    return undefined;
  }
  const match = navigator.userAgent.match(
    /(?:iPhone OS|CPU OS)\s+(\d+[._]\d+(?:[._]\d+)?)/,
  );
  if (!match) {
    return undefined;
  }
  return match[1].replace(/_/g, ".");
}

export function readRoute(): string {
  if (typeof window === "undefined") {
    return "";
  }
  return window.location.hash || "#/";
}

export function readAppVersion(): string | undefined {
  const version = document
    .querySelector('meta[name="ajax-app-version"]')
    ?.getAttribute("content")
    ?.trim();
  if (!version || version === "__AJAX_APP_VERSION__") {
    return undefined;
  }
  return version;
}

export function readHost(): string {
  if (typeof window === "undefined") {
    return "";
  }
  return window.location.hostname;
}

export function readRouteKind(): string {
  return parseRoute(readRoute()).kind;
}

export function readOnline(): boolean {
  if (typeof navigator === "undefined") {
    return true;
  }
  return navigator.onLine;
}

export function readVisibility(): DocumentVisibilityState {
  if (typeof document === "undefined") {
    return "visible";
  }
  return document.visibilityState;
}

export function readConnectionType(): string | undefined {
  if (typeof navigator === "undefined") {
    return undefined;
  }
  const connection = (navigator as Navigator & {
    connection?: { effectiveType?: string };
  }).connection;
  const effectiveType = connection?.effectiveType?.trim();
  return effectiveType || undefined;
}

export function readPixelRatio(): number {
  if (typeof window === "undefined") {
    return 0;
  }
  return Math.round(window.devicePixelRatio * 100) / 100;
}

export interface EventContext {
  event_id: string;
  session_id: string;
  install_id: string;
  sequence: number;
  app_version?: string;
  route: string;
  route_kind: string;
  host: string;
  online: boolean;
  visibility: DocumentVisibilityState;
  connection_type?: string;
  pixel_ratio: number;
  ios_version?: string;
  viewport_w: number;
  viewport_h: number;
  standalone: boolean;
}

export function buildEventContext(): EventContext {
  const { viewport_w, viewport_h } = readViewport();
  const app_version = readAppVersion();
  const ios_version = readIosVersion();
  const connection_type = readConnectionType();
  return {
    event_id: generateId(),
    session_id: getSessionId(),
    install_id: getInstallId(),
    sequence: nextSequence(),
    ...(app_version ? { app_version } : {}),
    route: readRoute(),
    route_kind: readRouteKind(),
    host: readHost(),
    online: readOnline(),
    visibility: readVisibility(),
    ...(connection_type ? { connection_type } : {}),
    pixel_ratio: readPixelRatio(),
    ...(ios_version ? { ios_version } : {}),
    viewport_w,
    viewport_h,
    standalone: isStandaloneDisplay(),
  };
}

/** Test seam: reset persisted identity and sequence between unit tests. */
export function resetTelemetryContextForTests(): void {
  try {
    localStorage.removeItem(INSTALL_ID_KEY);
    localStorage.removeItem(SEQUENCE_KEY);
    sessionStorage.removeItem(SESSION_ID_KEY);
  } catch {
    // ignore
  }
}
