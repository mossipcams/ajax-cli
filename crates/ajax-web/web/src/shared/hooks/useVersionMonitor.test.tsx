import { describe, it, expect, afterEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useVersionMonitor } from "./useVersionMonitor";

function jsonResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: () => Promise.resolve(JSON.stringify(body)),
  };
}

describe("useVersionMonitor", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("pins boot version on first success and does not raise the banner", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(jsonResponse({ version: "v1" }))));
    const { result } = renderHook(() => useVersionMonitor());

    expect(result.current.updateAvailable).toBe(false);

    await act(async () => {
      await result.current.checkVersion();
    });

    expect(result.current.updateAvailable).toBe(false);
  });

  it("sets updateAvailable when a later response has a different version", async () => {
    let versionCalls = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn(() => {
        versionCalls += 1;
        return Promise.resolve(jsonResponse({ version: versionCalls === 1 ? "v1" : "v2" }));
      }),
    );
    const { result } = renderHook(() => useVersionMonitor());

    await act(async () => {
      await result.current.checkVersion();
    });
    expect(result.current.updateAvailable).toBe(false);

    await act(async () => {
      await result.current.checkVersion();
    });
    expect(result.current.updateAvailable).toBe(true);
  });

  it("keeps updateAvailable true once set", async () => {
    let versionCalls = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn(() => {
        versionCalls += 1;
        const versions = ["v1", "v2", "v1"];
        return Promise.resolve(jsonResponse({ version: versions[versionCalls - 1] ?? "v1" }));
      }),
    );
    const { result } = renderHook(() => useVersionMonitor());

    await act(async () => {
      await result.current.checkVersion();
    });
    await act(async () => {
      await result.current.checkVersion();
    });
    expect(result.current.updateAvailable).toBe(true);

    await act(async () => {
      await result.current.checkVersion();
    });
    expect(result.current.updateAvailable).toBe(true);
  });

  it("ignores empty or missing version field", async () => {
    let call = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn(() => {
        call += 1;
        if (call === 1) return Promise.resolve(jsonResponse({ version: "" }));
        if (call === 2) return Promise.resolve(jsonResponse({}));
        if (call === 3) return Promise.resolve(jsonResponse({ version: "v1" }));
        return Promise.resolve(jsonResponse({ version: "v2" }));
      }),
    );
    const { result } = renderHook(() => useVersionMonitor());

    await act(async () => {
      await result.current.checkVersion();
    });
    expect(result.current.updateAvailable).toBe(false);

    await act(async () => {
      await result.current.checkVersion();
    });
    expect(result.current.updateAvailable).toBe(false);

    await act(async () => {
      await result.current.checkVersion();
    });
    expect(result.current.updateAvailable).toBe(false);

    await act(async () => {
      await result.current.checkVersion();
    });
    expect(result.current.updateAvailable).toBe(true);
  });

  it("swallows rejected fetch without state change", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.reject(new Error("offline"))));
    const { result } = renderHook(() => useVersionMonitor());

    await act(async () => {
      await expect(result.current.checkVersion()).resolves.toBeUndefined();
    });
    expect(result.current.updateAvailable).toBe(false);
  });

  it("keeps checkVersion referentially stable", () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(jsonResponse({ version: "v1" }))));
    const { result, rerender } = renderHook(() => useVersionMonitor());
    const first = result.current.checkVersion;
    rerender();
    rerender();
    expect(result.current.checkVersion).toBe(first);
  });
});
