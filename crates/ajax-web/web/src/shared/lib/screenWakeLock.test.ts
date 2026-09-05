import { afterEach, describe, expect, it, vi } from "vitest";
import {
  createBrowserScreenWakeLockPlatform,
  setupScreenWakeLock,
  type ScreenWakeLockPlatform,
  type WakeLockSentinel,
} from "./screenWakeLock";

function fakeSentinel(): WakeLockSentinel & { release: ReturnType<typeof vi.fn> } {
  const listeners = new Map<string, Set<() => void>>();
  return {
    release: vi.fn(async () => {}),
    addEventListener(type: "release", listener: () => void) {
      const set = listeners.get(type) ?? new Set();
      set.add(listener);
      listeners.set(type, set);
    },
    removeEventListener(type: "release", listener: () => void) {
      listeners.get(type)?.delete(listener);
    },
    emitRelease() {
      for (const listener of listeners.get("release") ?? []) listener();
    },
  };
}

function platform(overrides: Partial<ScreenWakeLockPlatform> = {}): {
  platform: ScreenWakeLockPlatform;
  requestWakeLock: ReturnType<typeof vi.fn>;
  visibilityHandlers: Set<() => void>;
  gestureHandlers: Set<() => void>;
  visible: boolean;
} {
  const requestWakeLock = vi.fn(async () => fakeSentinel());
  const visibilityHandlers = new Set<() => void>();
  const gestureHandlers = new Set<() => void>();
  let visible = true;
  const base: ScreenWakeLockPlatform = {
    isVisible: () => visible,
    requestWakeLock,
    addVisibilityListener(handler) {
      visibilityHandlers.add(handler);
      return () => visibilityHandlers.delete(handler);
    },
    addUserGestureListener(handler) {
      gestureHandlers.add(handler);
      return () => gestureHandlers.delete(handler);
    },
  };
  const platform: ScreenWakeLockPlatform = { ...base, ...overrides };
  return {
    platform,
    requestWakeLock: vi.mocked(platform.requestWakeLock),
    visibilityHandlers,
    gestureHandlers,
    get visible() {
      return visible;
    },
    set visible(next: boolean) {
      visible = next;
    },
  };
}

describe("setupScreenWakeLock", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("acquires when visible on setup", async () => {
    const ctx = platform();
    setupScreenWakeLock(ctx.platform);
    await Promise.resolve();
    expect(ctx.requestWakeLock).toHaveBeenCalledOnce();
  });

  it("releases when the document becomes hidden", async () => {
    const sentinel = fakeSentinel();
    const ctx = platform({
      requestWakeLock: vi.fn(async () => sentinel),
    });
    const cleanup = setupScreenWakeLock(ctx.platform);
    await Promise.resolve();
    expect(sentinel.release).not.toHaveBeenCalled();

    ctx.visible = false;
    for (const handler of ctx.visibilityHandlers) handler();
    await Promise.resolve();

    expect(sentinel.release).toHaveBeenCalledOnce();
    cleanup();
  });

  it("re-acquires when visibility returns to visible", async () => {
    const ctx = platform();
    const cleanup = setupScreenWakeLock(ctx.platform);
    await Promise.resolve();
    expect(ctx.requestWakeLock).toHaveBeenCalledTimes(1);

    ctx.visible = false;
    for (const handler of ctx.visibilityHandlers) handler();
    await Promise.resolve();

    ctx.visible = true;
    for (const handler of ctx.visibilityHandlers) handler();
    await Promise.resolve();

    expect(ctx.requestWakeLock).toHaveBeenCalledTimes(2);
    cleanup();
  });

  it("acquires on user gesture while visible", async () => {
    const ctx = platform({
      requestWakeLock: vi
        .fn()
        .mockRejectedValueOnce(new Error("gesture required"))
        .mockImplementation(async () => fakeSentinel()),
    });
    const cleanup = setupScreenWakeLock(ctx.platform);
    await Promise.resolve();
    expect(ctx.requestWakeLock).toHaveBeenCalledTimes(1);

    for (const handler of ctx.gestureHandlers) handler();
    await Promise.resolve();

    expect(ctx.requestWakeLock).toHaveBeenCalledTimes(2);
    cleanup();
  });

  it("fails open when wake lock is unsupported", async () => {
    const cleanup = setupScreenWakeLock(null);
    expect(cleanup).toBeTypeOf("function");
    cleanup();
  });

  it("fails open when requestWakeLock rejects", async () => {
    const ctx = platform({
      requestWakeLock: vi.fn(async () => {
        throw new Error("denied");
      }),
    });
    expect(() => setupScreenWakeLock(ctx.platform)).not.toThrow();
    await Promise.resolve();
    expect(ctx.requestWakeLock).toHaveBeenCalledOnce();
  });

  it("releases and stops listening on cleanup", async () => {
    const sentinel = fakeSentinel();
    const ctx = platform({
      requestWakeLock: vi.fn(async () => sentinel),
    });
    const cleanup = setupScreenWakeLock(ctx.platform);
    await Promise.resolve();

    cleanup();
    await Promise.resolve();
    expect(sentinel.release).toHaveBeenCalledOnce();
    expect(ctx.visibilityHandlers.size).toBe(0);
    expect(ctx.gestureHandlers.size).toBe(0);
  });
});

describe("createBrowserScreenWakeLockPlatform", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("returns null when navigator.wakeLock is missing", () => {
    vi.stubGlobal("navigator", {});
    vi.stubGlobal("document", { visibilityState: "visible" });
    expect(createBrowserScreenWakeLockPlatform()).toBeNull();
  });

  it("returns a platform when navigator.wakeLock is available", () => {
    const request = vi.fn();
    vi.stubGlobal("navigator", { wakeLock: { request } });
    vi.stubGlobal("document", {
      visibilityState: "visible",
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });
    expect(createBrowserScreenWakeLockPlatform()).not.toBeNull();
  });
});
