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
    const response = await fetch(path, { cache: "no-store", credentials: "same-origin" });
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
    browser_mode: "Safari/browser",
    backend_url: window.location.origin,
    navigator_onLine: navigator.onLine,
    app_version: loadedAppVersion,
    location: window.location.href,
    checks,
  };
}

/** Copy to clipboard; returns true when the native clipboard accepted it. */
export async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // NotAllowedError when backgrounded on iOS, SecurityError in some contexts.
  }
  return false;
}
