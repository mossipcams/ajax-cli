import { describe, it, expect, afterEach, vi } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { useRuntimeControl } from "./useRuntimeControl";

function jsonResponse(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    text: () => Promise.resolve(JSON.stringify(body)),
  };
}

const runtimeFixture = {
  ok: true,
  version: "0.11.0",
  commit: "abc123",
  profile: "stable",
  uptime_seconds: 120,
  update_available: { known: true, available: false },
  operation: null,
  logs: [],
  test_in_stable: true,
};

describe("useRuntimeControl", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("refresh catches fetch failures without unhandled rejections or operation errors", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/server/runtime") {
          return Promise.reject(new Error("network down"));
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    const { result } = renderHook(() => useRuntimeControl());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.status).toBeNull();
    expect(result.current.error).toBeNull();

    await act(async () => {
      await result.current.refresh();
    });

    expect(result.current.loading).toBe(false);
    expect(result.current.status).toBeNull();
    expect(result.current.error).toBeNull();
  });

  it("refresh updates status on success", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        const path = String(input);
        if (path === "/api/server/runtime") {
          return Promise.resolve(jsonResponse(runtimeFixture));
        }
        return Promise.reject(new Error(`unexpected fetch: ${path}`));
      }),
    );

    const { result } = renderHook(() => useRuntimeControl());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.status).toMatchObject({ version: "0.11.0" });
  });
});
