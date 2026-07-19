import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { diagnosticFetch, buildDiagnosticsReport } from "./diagnostics";

describe("diagnosticFetch", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn());
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns ok result for a successful JSON response", async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      text: () => Promise.resolve('{"version":"0.1"}'),
    });
    vi.stubGlobal("fetch", mockFetch);

    const result = await diagnosticFetch("/api/version");
    expect(mockFetch).toHaveBeenCalledWith("/api/version", {
      cache: "no-store",
      credentials: "same-origin",
    });
    expect(result.ok).toBe(true);
    expect(result.status).toBe(200);
    expect(result.error).toBeNull();
    expect(result.body).toBe(JSON.stringify({ version: "0.1" }, null, 2));
  });

  it("returns ok=false for a non-2xx response", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: false,
        status: 503,
        text: () => Promise.resolve("Service Unavailable"),
      }),
    );

    const result = await diagnosticFetch("/api/health");
    expect(result.ok).toBe(false);
    expect(result.status).toBe(503);
  });

  it("captures network errors", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockRejectedValue(new Error("Failed to fetch")),
    );

    const result = await diagnosticFetch("/api/health");
    expect(result.ok).toBe(false);
    expect(result.status).toBeNull();
    expect(result.error).toContain("Failed to fetch");
  });
});

describe("buildDiagnosticsReport", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      text: () => Promise.resolve('{"ok":true}'),
    }));
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("reports the supported Safari/browser shell without service worker state", async () => {
    const report = await buildDiagnosticsReport();

    expect(report.browser_mode).toBe("Safari/browser");
    expect(report).not.toHaveProperty("service_worker_controller");
    expect(report).not.toHaveProperty("terminal_surface_v2");
    expect(report).not.toHaveProperty("surface_v2_last_error");
  });
});
