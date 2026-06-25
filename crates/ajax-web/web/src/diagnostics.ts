// Diagnostics report builder and clipboard helper. Same-origin probes only;
// the report is a display convenience for connection debugging.

export interface DiagnosticCheck {
  ok: boolean;
  status: number | null;
  error: string | null;
  body: string | null;
}

export async function diagnosticFetch(path: string): Promise<DiagnosticCheck> {
  try {
    const response = await fetch(path, { cache: "no-store" });
    const text = await response.text();
    let body = text.slice(0, 600);
    try {
      body = JSON.stringify(JSON.parse(text), null, 2).slice(0, 600);
    } catch {
      // Plain-text responses are still useful diagnostics.
    }
    return { ok: response.ok, status: response.status, error: null, body };
  } catch (error) {
    return {
      ok: false,
      status: null,
      error: error instanceof Error ? error.message : String(error),
      body: null,
    };
  }
}

export function isStandalonePwa(): boolean {
  return (
    window.matchMedia?.("(display-mode: standalone)").matches === true ||
    (window.navigator as { standalone?: boolean }).standalone === true
  );
}

export async function buildDiagnosticsReport(
  detailHandle?: string | null,
): Promise<Record<string, unknown>> {
  const checks: Record<string, DiagnosticCheck> = {
    health: await diagnosticFetch("/api/health"),
    version: await diagnosticFetch("/api/version"),
    cockpit: await diagnosticFetch("/api/cockpit"),
  };
  if (detailHandle) {
    checks.task = await diagnosticFetch(`/api/tasks/${encodeURIComponent(detailHandle)}`);
  }

  const loadedAppVersion =
    document.querySelector<HTMLMetaElement>('meta[name="ajax-app-version"]')?.content ?? null;

  return {
    browser_mode: isStandalonePwa() ? "standalone" : "Safari/browser",
    backend_url: window.location.origin,
    navigator_onLine: navigator.onLine,
    app_version: loadedAppVersion,
    service_worker_controller: Boolean(navigator.serviceWorker?.controller),
    location: window.location.href,
    checks,
  };
}

/** Copy to clipboard; returns true when the native clipboard accepted it. */
export async function copyText(text: string): Promise<boolean> {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return true;
  }
  return false;
}

/** Unregister any leftover service worker from old PWA installs. The new client
 * never registers one; this only cleans up the past. */
export function unregisterExistingServiceWorkers(): void {
  if (!("serviceWorker" in navigator)) return;
  navigator.serviceWorker
    .getRegistrations()
    .then((registrations) => Promise.all(registrations.map((r) => r.unregister())))
    .catch(() => {
      /* cleanup is best-effort */
    });
}
