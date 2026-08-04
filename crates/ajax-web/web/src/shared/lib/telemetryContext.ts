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

function generateId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
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

export interface EventContext {
  event_id: string;
  session_id: string;
  install_id: string;
  sequence: number;
  app_version?: string;
  route: string;
  ios_version?: string;
  viewport_w: number;
  viewport_h: number;
  standalone: boolean;
}

export function buildEventContext(): EventContext {
  const { viewport_w, viewport_h } = readViewport();
  const app_version = readAppVersion();
  const ios_version = readIosVersion();
  return {
    event_id: generateId(),
    session_id: getSessionId(),
    install_id: getInstallId(),
    sequence: nextSequence(),
    ...(app_version ? { app_version } : {}),
    route: readRoute(),
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
