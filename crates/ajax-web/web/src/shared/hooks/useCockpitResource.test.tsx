import { describe, it, expect, afterEach, vi, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import cockpitFixture from "@/fixtures/cockpit.json";
import type { BrowserCockpitView } from "@/shared/lib/types";
import { ApiError } from "@/shared/lib/api";
import { useCockpitResource } from "./useCockpitResource";

const cockpit = cockpitFixture as BrowserCockpitView;

const fetchCockpit = vi.fn<() => Promise<BrowserCockpitView>>();

vi.mock("@/shared/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/shared/lib/api")>();
  return {
    ...actual,
    fetchCockpit: () => fetchCockpit(),
  };
});

describe("useCockpitResource", () => {
  beforeEach(() => {
    fetchCockpit.mockReset();
    Object.defineProperty(document, "hidden", {
      configurable: true,
      value: false,
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("starts in loading with checking connection", () => {
    const { result } = renderHook(() => useCockpitResource());
    expect(result.current.cockpit).toEqual({
      status: "loading",
      data: null,
      error: null,
    });
    expect(result.current.connection).toBe("checking");
    expect(result.current.connectionDetail).toBeNull();
  });

  // An iOS home-screen PWA mounts with document.hidden still true behind the
  // splash screen. loadCockpit must fetch anyway, or the app strands on
  // "checking" until the 60s hidden interval fires. Skipping while hidden is
  // the background poll's job (App.tsx), not this loader's.
  it("loadCockpit still fetches when document.hidden is true", async () => {
    Object.defineProperty(document, "hidden", {
      configurable: true,
      value: true,
    });
    fetchCockpit.mockResolvedValue(cockpit);
    const { result } = renderHook(() => useCockpitResource());

    await act(async () => {
      await result.current.loadCockpit();
    });

    expect(fetchCockpit).toHaveBeenCalledTimes(1);
    expect(result.current.cockpit.status).toBe("ready");
    expect(result.current.connection).toBe("connected");
  });

  it("collapses concurrent loadCockpit calls with the in-flight guard", async () => {
    let resolve!: (value: BrowserCockpitView) => void;
    const pending = new Promise<BrowserCockpitView>((res) => {
      resolve = res;
    });
    fetchCockpit.mockReturnValue(pending);
    const { result } = renderHook(() => useCockpitResource());

    let first!: Promise<void>;
    let second!: Promise<void>;
    await act(async () => {
      first = result.current.loadCockpit();
      second = result.current.loadCockpit();
    });

    expect(fetchCockpit).toHaveBeenCalledTimes(1);

    await act(async () => {
      resolve(cockpit);
      await first;
      await second;
    });

    expect(fetchCockpit).toHaveBeenCalledTimes(1);
    expect(result.current.cockpit.status).toBe("ready");
  });

  it("schedules one trailing fetch when loadCockpit is called with trailing while in flight", async () => {
    let resolve!: (value: BrowserCockpitView) => void;
    const pending = new Promise<BrowserCockpitView>((res) => {
      resolve = res;
    });
    fetchCockpit.mockReturnValueOnce(pending).mockResolvedValue(cockpit);
    const { result } = renderHook(() => useCockpitResource());

    let first!: Promise<void>;
    let second!: Promise<void>;
    await act(async () => {
      first = result.current.loadCockpit();
      second = result.current.loadCockpit({ trailing: true });
    });

    expect(fetchCockpit).toHaveBeenCalledTimes(1);

    await act(async () => {
      resolve(cockpit);
      await first;
      await second;
    });

    await vi.waitFor(() => expect(fetchCockpit).toHaveBeenCalledTimes(2));
    expect(result.current.cockpit.status).toBe("ready");
  });

  it("maps a successful poll to ready and connected", async () => {
    fetchCockpit.mockResolvedValue(cockpit);
    const { result } = renderHook(() => useCockpitResource());

    await act(async () => {
      await result.current.loadCockpit();
    });

    expect(result.current.cockpit).toEqual({
      status: "ready",
      data: cockpit,
      error: null,
    });
    expect(result.current.connection).toBe("connected");
    expect(result.current.connectionDetail).toBeNull();
  });

  it("suppresses cockpit updates when the projection is unchanged", async () => {
    fetchCockpit.mockResolvedValue(cockpit);
    const { result } = renderHook(() => useCockpitResource());

    await act(async () => {
      await result.current.loadCockpit();
    });
    const readyCockpit = result.current.cockpit;

    await act(async () => {
      await result.current.loadCockpit();
    });

    expect(result.current.cockpit).toBe(readyCockpit);
    expect(result.current.connection).toBe("connected");
  });

  it("maps poll failure with existing data to stale while keeping data", async () => {
    fetchCockpit.mockResolvedValueOnce(cockpit).mockRejectedValueOnce(
      new ApiError("http", "HTTP 503", 503),
    );
    const { result } = renderHook(() => useCockpitResource());

    await act(async () => {
      await result.current.loadCockpit();
    });
    await act(async () => {
      await result.current.loadCockpit();
    });

    expect(result.current.cockpit.status).toBe("stale");
    expect(result.current.cockpit.data).toEqual(cockpit);
    expect(result.current.cockpit.error).toMatchObject({ kind: "http", message: "HTTP 503" });
    expect(result.current.connection).toBe("disconnected");
    expect(result.current.connectionDetail).toBe("HTTP 503");
  });

  it("maps first poll failure with no data to error", async () => {
    fetchCockpit.mockRejectedValue(new ApiError("http", "HTTP 503", 503));
    const { result } = renderHook(() => useCockpitResource());

    await act(async () => {
      await result.current.loadCockpit();
    });

    expect(result.current.cockpit).toEqual({
      status: "error",
      data: null,
      error: expect.objectContaining({ kind: "http", message: "HTTP 503" }),
    });
    expect(result.current.connection).toBe("disconnected");
  });

  it("maps ApiError kinds to connection states", async () => {
    const cases = [
      {
        error: new ApiError("network", "Failed to fetch"),
        connection: "backend unreachable" as const,
      },
      {
        error: new ApiError("stale-session", "HTTP 401", 401),
        connection: "stale session" as const,
      },
      {
        error: new ApiError("http", "HTTP 500", 500),
        connection: "disconnected" as const,
      },
    ];

    for (const { error, connection } of cases) {
      fetchCockpit.mockRejectedValueOnce(error);
      const { result } = renderHook(() => useCockpitResource());
      await act(async () => {
        await result.current.loadCockpit();
      });
      expect(result.current.connection).toBe(connection);
      expect(result.current.connectionDetail).toBe(error.message);
    }
  });

  it("maps non-ApiError throws to backend unreachable with message", async () => {
    fetchCockpit.mockRejectedValue(new Error("offline"));
    const { result } = renderHook(() => useCockpitResource());

    await act(async () => {
      await result.current.loadCockpit();
    });

    expect(result.current.connection).toBe("backend unreachable");
    expect(result.current.connectionDetail).toBe("offline");
    expect(result.current.cockpit.status).toBe("error");
  });

  it("applyCockpit sets ready, connected, and clears connection detail", () => {
    const { result } = renderHook(() => useCockpitResource());

    act(() => {
      result.current.applyConnectionError(new ApiError("http", "HTTP 500", 500));
    });
    act(() => {
      result.current.applyCockpit(cockpit);
    });

    expect(result.current.cockpit).toEqual({
      status: "ready",
      data: cockpit,
      error: null,
    });
    expect(result.current.connection).toBe("connected");
    expect(result.current.connectionDetail).toBeNull();
  });

  it("applyConnectionError updates connection without changing cockpit resource status", async () => {
    fetchCockpit.mockResolvedValue(cockpit);
    const { result } = renderHook(() => useCockpitResource());

    await act(async () => {
      await result.current.loadCockpit();
    });
    const readyCockpit = result.current.cockpit;

    act(() => {
      result.current.applyConnectionError(new ApiError("http", "HTTP 500", 500));
    });

    expect(result.current.cockpit).toBe(readyCockpit);
    expect(result.current.connection).toBe("disconnected");
    expect(result.current.connectionDetail).toBe("HTTP 500");
  });

  it("recovers stale cockpit to ready after a successful applyCockpit with unchanged data", async () => {
    fetchCockpit.mockResolvedValueOnce(cockpit).mockRejectedValueOnce(
      new ApiError("http", "HTTP 503", 503),
    );
    const { result } = renderHook(() => useCockpitResource());

    await act(async () => {
      await result.current.loadCockpit();
    });
    await act(async () => {
      await result.current.loadCockpit();
    });
    expect(result.current.cockpit.status).toBe("stale");

    act(() => {
      result.current.applyCockpit(structuredClone(cockpit));
    });

    expect(result.current.cockpit.status).toBe("ready");
    expect(result.current.cockpit.data).toEqual(cockpit);
    expect(result.current.connection).toBe("connected");
  });

  it("keeps loadCockpit, applyCockpit, and applyConnectionError referentially stable", () => {
    const { result, rerender } = renderHook(() => useCockpitResource());
    const firstLoad = result.current.loadCockpit;
    const firstApply = result.current.applyCockpit;
    const firstError = result.current.applyConnectionError;
    rerender();
    rerender();
    expect(result.current.loadCockpit).toBe(firstLoad);
    expect(result.current.applyCockpit).toBe(firstApply);
    expect(result.current.applyConnectionError).toBe(firstError);
  });
});
