import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { diagnosticFetch, buildDiagnosticsReport, copyText } from "./diagnostics";

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
    expect(result.body).toContain("version");
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
  });

  it("includes terminal_surface_v2 and surface_v2_last_error", async () => {
    localStorage.setItem("ajax.terminal.surfaceV2", "true");
    sessionStorage.setItem("ajax.terminal.surfaceV2.lastError", "boot timeout");

    const report = await buildDiagnosticsReport();

    expect(report.terminal_surface_v2).toBe(true);
    expect(report.surface_v2_last_error).toBe("boot timeout");
  });
});

describe("copyText", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns true when clipboard accepts the write", async () => {
    vi.stubGlobal("navigator", {
      clipboard: { writeText: vi.fn().mockResolvedValue(undefined) },
    });

    const ok = await copyText("hello");
    expect(ok).toBe(true);
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("hello");
  });

  it("returns true via execCommand when clipboard API is unavailable", async () => {
    vi.stubGlobal("navigator", {});
    const execCommand = vi.fn().mockReturnValue(true);
    Object.defineProperty(document, "execCommand", {
      value: execCommand,
      configurable: true,
    });
    const textareasBefore = document.body.querySelectorAll("textarea").length;

    const ok = await copyText("plain-http copy");

    expect(ok).toBe(true);
    expect(execCommand).toHaveBeenCalledWith("copy");
    expect(document.body.querySelectorAll("textarea").length).toBe(textareasBefore);
    Reflect.deleteProperty(document, "execCommand");
  });

  it("returns false when clipboard API is unavailable", async () => {
    vi.stubGlobal("navigator", {});

    const ok = await copyText("hello");
    expect(ok).toBe(false);
  });
});
