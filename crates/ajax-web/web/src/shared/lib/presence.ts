import { AJAX_PROTECTED_HEADERS } from "./api";

export const PRESENCE_HEARTBEAT_MS = 15_000;

const PRESENCE_PATH = "/api/presence";

function presenceBody(visible: boolean): string {
  return JSON.stringify({ visible });
}

function postVisible(visible: boolean): void {
  void fetch(PRESENCE_PATH, {
    method: "POST",
    cache: "no-store",
    credentials: "same-origin",
    headers: { ...AJAX_PROTECTED_HEADERS, "content-type": "application/json" },
    body: presenceBody(visible),
  }).catch(() => {});
}

function postHidden(): void {
  const body = presenceBody(false);
  if (typeof navigator !== "undefined" && typeof navigator.sendBeacon === "function") {
    const queued = navigator.sendBeacon(
      PRESENCE_PATH,
      new Blob([body], { type: "application/json" }),
    );
    if (queued) return;
  }
  void fetch(PRESENCE_PATH, {
    method: "POST",
    keepalive: true,
    cache: "no-store",
    credentials: "same-origin",
    headers: { ...AJAX_PROTECTED_HEADERS, "content-type": "application/json" },
    body,
  }).catch(() => {});
}

export function setupPresence(): () => void {
  if (typeof document === "undefined") {
    return () => {};
  }

  let heartbeat: ReturnType<typeof setInterval> | null = null;
  let reportedVisible = false;

  const stopHeartbeat = () => {
    if (heartbeat === null) return;
    clearInterval(heartbeat);
    heartbeat = null;
  };

  const reportVisible = () => {
    reportedVisible = true;
    postVisible(true);
  };

  const reportHidden = () => {
    stopHeartbeat();
    if (!reportedVisible) return;
    reportedVisible = false;
    postHidden();
  };

  const onVisibilityChange = () => {
    if (document.visibilityState === "visible") {
      reportVisible();
      stopHeartbeat();
      heartbeat = setInterval(reportVisible, PRESENCE_HEARTBEAT_MS);
    } else {
      reportHidden();
    }
  };

  onVisibilityChange();
  document.addEventListener("visibilitychange", onVisibilityChange);
  window.addEventListener("pagehide", reportHidden);

  return () => {
    document.removeEventListener("visibilitychange", onVisibilityChange);
    window.removeEventListener("pagehide", reportHidden);
    stopHeartbeat();
  };
}
