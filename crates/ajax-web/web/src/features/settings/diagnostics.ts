import { AJAX_PROTECTED_HEADERS, fetchProtectedWithSessionRenewal } from "@/shared/lib/api";

export interface DiagnosticCheck {
  ok: boolean;
  status: number | null;
  error: string | null;
  body: string | null;
}

const PUBLIC_DIAGNOSTIC_PATHS = new Set(["/api/health"]);

export async function diagnosticFetch(path: string): Promise<DiagnosticCheck> {
  try {
    const response = PUBLIC_DIAGNOSTIC_PATHS.has(path)
      ? await fetch(path, { cache: "no-store", credentials: "same-origin" })
      : await fetchProtectedWithSessionRenewal(path, {
          cache: "no-store",
          credentials: "same-origin",
          headers: { ...AJAX_PROTECTED_HEADERS },
        });
    const text = await response.text();
    let body = text.slice(0, 600);
    try {
      body = JSON.stringify(JSON.parse(text), null, 2).slice(0, 600);
    } catch {
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

export interface DiagnosticsReport {
  browser_mode: string;
  backend_url: string;
  navigator_onLine: boolean;
  app_version: string | null;
  location: string;
  checks: Record<string, DiagnosticCheck>;
}

export interface DiagnosticCheckSummary {
  name: string;
  ok: boolean;
  detail: string;
}

export async function buildDiagnosticsReport(
  detailHandle?: string | null,
): Promise<DiagnosticsReport> {
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

export function summarizeDiagnosticChecks(
  checks: Record<string, DiagnosticCheck>,
): DiagnosticCheckSummary[] {
  return Object.entries(checks).map(([name, check]) => ({
    name,
    ok: check.ok,
    detail: check.ok ? "ok" : (check.error ?? `HTTP ${check.status}`),
  }));
}
