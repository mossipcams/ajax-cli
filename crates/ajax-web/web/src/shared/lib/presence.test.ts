import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setupPresence, PRESENCE_HEARTBEAT_MS } from "./presence";

type VisibilityState = "visible" | "hidden";

function setVisibility(state: VisibilityState): void {
  Object.defineProperty(document, "visibilityState", {
    configurable: true,
    get: () => state,
  });
  document.dispatchEvent(new Event("visibilitychange"));
}

function presenceCalls(fetchMock: ReturnType<typeof vi.fn>): unknown[][] {
  return fetchMock.mock.calls.filter(([path]) => path === "/api/presence");
}

function fetchBodies(fetchMock: ReturnType<typeof vi.fn>): string[] {
  return presenceCalls(fetchMock).map(([, init]) => (init as RequestInit).body as string);
}

describe("setupPresence", () => {
  let fetchMock: ReturnType<typeof vi.fn>;
  let beaconMock: ReturnType<typeof vi.fn>;
  let cleanup: (() => void) | null = null;

  beforeEach(() => {
    vi.useFakeTimers();
    setVisibility("visible");
    fetchMock = vi.fn(async () => new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    beaconMock = vi.fn(() => true);
    Object.defineProperty(navigator, "sendBeacon", {
      configurable: true,
      value: beaconMock,
    });
  });

  afterEach(() => {
    cleanup?.();
    cleanup = null;
    vi.useRealTimers();
    vi.unstubAllGlobals();
    delete (navigator as { sendBeacon?: unknown }).sendBeacon;
    Object.defineProperty(document, "visibilityState", {
      configurable: true,
      get: () => "visible",
    });
  });

  it("reports visible=true immediately when started on a visible page", () => {
    cleanup = setupPresence();

    expect(presenceCalls(fetchMock)).toHaveLength(1);
    expect(fetchBodies(fetchMock)).toEqual(['{"visible":true}']);
    const [, init] = presenceCalls(fetchMock)[0] as [string, RequestInit];
    expect(init.method).toBe("POST");
    expect(init.credentials).toBe("same-origin");
    expect(init.headers).toMatchObject({
      "X-Requested-With": "XMLHttpRequest",
      "content-type": "application/json",
    });
  });

  it("re-reports visible=true every heartbeat interval while visible", () => {
    cleanup = setupPresence();

    vi.advanceTimersByTime(PRESENCE_HEARTBEAT_MS - 1);
    expect(presenceCalls(fetchMock)).toHaveLength(1);

    vi.advanceTimersByTime(1);
    expect(presenceCalls(fetchMock)).toHaveLength(2);

    vi.advanceTimersByTime(PRESENCE_HEARTBEAT_MS * 2);
    expect(fetchBodies(fetchMock)).toEqual(Array(4).fill('{"visible":true}'));
  });

  it("uses a 15 second heartbeat", () => {
    cleanup = setupPresence();
    vi.advanceTimersByTime(15_000);
    expect(PRESENCE_HEARTBEAT_MS).toBe(15_000);
    expect(presenceCalls(fetchMock)).toHaveLength(2);
  });

  it("reports visible=true on visibilitychange to visible and restarts the heartbeat", () => {
    setVisibility("hidden");
    cleanup = setupPresence();
    expect(presenceCalls(fetchMock)).toHaveLength(0);

    vi.advanceTimersByTime(PRESENCE_HEARTBEAT_MS * 3);
    expect(presenceCalls(fetchMock)).toHaveLength(0);

    setVisibility("visible");
    expect(presenceCalls(fetchMock)).toHaveLength(1);

    vi.advanceTimersByTime(PRESENCE_HEARTBEAT_MS);
    expect(presenceCalls(fetchMock)).toHaveLength(2);
  });

  it("sends exactly one visible=false beacon immediately on hide and stops heartbeating", () => {
    cleanup = setupPresence();
    fetchMock.mockClear();

    setVisibility("hidden");

    expect(beaconMock).toHaveBeenCalledTimes(1);
    const [path, blob] = beaconMock.mock.calls[0] as [string, Blob];
    expect(path).toBe("/api/presence");
    expect(blob).toBeInstanceOf(Blob);
    expect(blob.type).toBe("application/json");
    expect(blob.size).toBe('{"visible":false}'.length);
    expect(presenceCalls(fetchMock)).toHaveLength(0);

    vi.advanceTimersByTime(PRESENCE_HEARTBEAT_MS * 4);
    expect(presenceCalls(fetchMock)).toHaveLength(0);
    expect(beaconMock).toHaveBeenCalledTimes(1);
  });

  it("does not send a second visible=false on pagehide after the hide already reported", () => {
    cleanup = setupPresence();

    setVisibility("hidden");
    window.dispatchEvent(new Event("pagehide"));

    expect(beaconMock).toHaveBeenCalledTimes(1);
  });

  it("sends visible=false on pagehide while the page is still visible", () => {
    cleanup = setupPresence();

    window.dispatchEvent(new Event("pagehide"));

    expect(beaconMock).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(PRESENCE_HEARTBEAT_MS * 2);
    expect(presenceCalls(fetchMock)).toHaveLength(1);
  });

  it("falls back to fetch keepalive with protected headers when sendBeacon is unavailable", () => {
    delete (navigator as { sendBeacon?: unknown }).sendBeacon;
    cleanup = setupPresence();
    fetchMock.mockClear();

    setVisibility("hidden");

    expect(presenceCalls(fetchMock)).toHaveLength(1);
    const [, init] = presenceCalls(fetchMock)[0] as [string, RequestInit];
    expect(init.body).toBe('{"visible":false}');
    expect(init.keepalive).toBe(true);
    expect(init.method).toBe("POST");
    expect(init.credentials).toBe("same-origin");
    expect(init.headers).toMatchObject({ "X-Requested-With": "XMLHttpRequest" });
  });

  it("falls back to fetch keepalive when sendBeacon refuses the report", () => {
    beaconMock.mockReturnValue(false);
    cleanup = setupPresence();
    fetchMock.mockClear();

    setVisibility("hidden");

    expect(beaconMock).toHaveBeenCalledTimes(1);
    expect(fetchBodies(fetchMock)).toEqual(['{"visible":false}']);
    const [, init] = presenceCalls(fetchMock)[0] as [string, RequestInit];
    expect(init.keepalive).toBe(true);
  });

  it("does nothing while hidden at start and until the page becomes visible", () => {
    setVisibility("hidden");
    cleanup = setupPresence();

    window.dispatchEvent(new Event("pagehide"));

    expect(fetchMock).not.toHaveBeenCalled();
    expect(beaconMock).not.toHaveBeenCalled();
  });

  it("stops the heartbeat and removes listeners on cleanup", () => {
    const stop = setupPresence();
    fetchMock.mockClear();

    stop();
    vi.advanceTimersByTime(PRESENCE_HEARTBEAT_MS * 3);
    setVisibility("hidden");
    window.dispatchEvent(new Event("pagehide"));

    expect(fetchMock).not.toHaveBeenCalled();
    expect(beaconMock).not.toHaveBeenCalled();
  });
});
