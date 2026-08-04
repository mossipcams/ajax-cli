import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import posthog from "posthog-js";
import { resetTelemetryContextForTests } from "./telemetryContext";
import { flushTelemetryQueue } from "./telemetryUpload";
import {
  DEFAULT_POSTHOG_PROJECT_KEY,
  beginInteraction,
  captureEvent,
  capturePwaLaunch,
  capturePwaResume,
  captureRouteVisible,
  captureSwipe,
  captureTelemetryDiagnostic,
  createMemoryTelemetryStore,
  endTapToFeedback,
  endTapToOperationComplete,
  getTelemetryQueueStatus,
  initTelemetry,
  isNavigationPending,
  isTelemetryInitialized,
  markNavigationStart,
  readAppVersion,
  resetTelemetryForTests,
  setTelemetryStoreForTests,
  telemetryDistinctId,
  track,
} from "./telemetry";

vi.mock("posthog-js", () => ({
  default: {
    init: vi.fn(),
    identify: vi.fn(),
    capture: vi.fn(),
  },
}));

const mockedPosthog = vi.mocked(posthog);

async function drainTelemetry(): Promise<void> {
  await vi.waitFor(() => {
    expect(mockedPosthog.capture).toHaveBeenCalled();
  });
  await new Promise((resolve) => setTimeout(resolve, 0));
}

beforeEach(() => {
  resetTelemetryForTests();
  resetTelemetryContextForTests();
  localStorage.clear();
  sessionStorage.clear();
  mockedPosthog.init.mockReset();
  mockedPosthog.identify.mockReset();
  mockedPosthog.capture.mockReset();
  document.head.innerHTML = "";
  vi.stubEnv("VITE_POSTHOG_KEY", "phc_test_key");
  vi.stubEnv("VITE_POSTHOG_HOST", "");
  setTelemetryStoreForTests(createMemoryTelemetryStore());
});

afterEach(() => {
  resetTelemetryForTests();
  resetTelemetryContextForTests();
  localStorage.clear();
  sessionStorage.clear();
  vi.unstubAllEnvs();
});

describe("readAppVersion", () => {
  it("reads ajax-app-version when substituted", () => {
    document.head.innerHTML =
      '<meta name="ajax-app-version" content="1.2.3-test">';
    expect(readAppVersion()).toBe("1.2.3-test");
  });

  it("ignores the build placeholder", () => {
    document.head.innerHTML =
      '<meta name="ajax-app-version" content="__AJAX_APP_VERSION__">';
    expect(readAppVersion()).toBeUndefined();
  });
});

describe("initTelemetry", () => {
  it("no-ops when VITE_POSTHOG_KEY explicitly disables telemetry", () => {
    vi.stubEnv("VITE_POSTHOG_KEY", "off");

    initTelemetry();

    expect(mockedPosthog.init).not.toHaveBeenCalled();
    expect(isTelemetryInitialized()).toBe(false);
  });

  it("falls back to the default project write key when env is unset", () => {
    vi.stubEnv("VITE_POSTHOG_KEY", "");

    initTelemetry();

    expect(mockedPosthog.init).toHaveBeenCalledWith(
      DEFAULT_POSTHOG_PROJECT_KEY,
      expect.objectContaining({
        api_host: "https://us.i.posthog.com",
        disable_session_recording: true,
      }),
    );
    expect(isTelemetryInitialized()).toBe(true);
  });

  it("initializes PostHog Cloud with env key, TTFB vitals, and replay off", () => {
    document.head.innerHTML =
      '<meta name="ajax-app-version" content="9.8.7">';

    initTelemetry();

    expect(mockedPosthog.init).toHaveBeenCalledTimes(1);
    expect(mockedPosthog.init).toHaveBeenCalledWith("phc_test_key", {
      api_host: "https://us.i.posthog.com",
      defaults: "2026-05-30",
      autocapture: {
        css_selector_ignorelist: expect.arrayContaining([
          ".ph-no-autocapture",
          "[data-ph-no-autocapture]",
          "[data-sensitive]",
          ".terminal-host",
        ]),
      },
      capture_performance: {
        web_vitals: true,
        web_vitals_allowed_metrics: ["LCP", "CLS", "FCP", "INP", "TTFB"],
      },
      disable_session_recording: true,
      capture_exceptions: false,
    });
    expect(isTelemetryInitialized()).toBe(true);
  });

  it("uses VITE_POSTHOG_HOST when provided", () => {
    vi.stubEnv("VITE_POSTHOG_HOST", "https://eu.i.posthog.com");

    initTelemetry();

    expect(mockedPosthog.init).toHaveBeenCalledWith(
      "phc_test_key",
      expect.objectContaining({ api_host: "https://eu.i.posthog.com" }),
    );
  });

  it("identifies the operator surface with host metadata", () => {
    document.head.innerHTML =
      '<meta name="ajax-app-version" content="4.5.6">';

    initTelemetry();

    expect(mockedPosthog.identify).toHaveBeenCalledWith(telemetryDistinctId(), {
      host: window.location.hostname,
      origin: window.location.origin,
      app_version: "4.5.6",
    });
  });

  it("only initializes once", () => {
    initTelemetry();
    initTelemetry();

    expect(mockedPosthog.init).toHaveBeenCalledTimes(1);
    expect(mockedPosthog.identify).toHaveBeenCalledTimes(1);
  });

  it("fails soft when PostHog throws", () => {
    mockedPosthog.init.mockImplementation(() => {
      throw new Error("network down");
    });
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});

    expect(() => initTelemetry()).not.toThrow();
    expect(warn).toHaveBeenCalled();
    expect(isTelemetryInitialized()).toBe(false);

    warn.mockRestore();
  });
});

describe("track", () => {
  it("falls back to direct capture when no store is available", async () => {
    setTelemetryStoreForTests(null);
    // Force ensureStore to treat IndexedDB as missing.
    const originalIdb = globalThis.indexedDB;
    // @ts-expect-error test seam
    delete globalThis.indexedDB;
    initTelemetry();
    track("ajax_swipe", { direction: "left", duration_ms: 50 });
    await drainTelemetry();
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_swipe",
      expect.objectContaining({ direction: "left", duration_ms: 50 }),
    );
    globalThis.indexedDB = originalIdb;
  });

  it("no-ops before init", () => {
    vi.stubEnv("VITE_POSTHOG_KEY", "off");
    initTelemetry();
    track("ajax_swipe", { direction: "left" });
    expect(mockedPosthog.capture).not.toHaveBeenCalled();
  });

  it("persists then enriches events with context and forwards to posthog.capture", async () => {
    const backing = new Map();
    const store = createMemoryTelemetryStore(backing);
    setTelemetryStoreForTests(store);
    initTelemetry();
    track("ajax_swipe", { direction: "left", duration_ms: 120 });
    await drainTelemetry();
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_swipe",
      expect.objectContaining({
        direction: "left",
        duration_ms: 120,
        event_id: expect.any(String),
        session_id: expect.any(String),
        install_id: expect.any(String),
        sequence: expect.any(Number),
        route: expect.any(String),
        viewport_w: expect.any(Number),
        viewport_h: expect.any(Number),
        standalone: expect.any(Boolean),
      }),
    );
    expect(await store.countPending()).toBe(0);
  });

  it("records tap-to-feedback and tap-to-operation-complete durations", async () => {
    initTelemetry();
    const id = beginInteraction("drop");
    endTapToFeedback(id, "confirm");
    endTapToOperationComplete(id, { ok: true, op: "drop" });
    await drainTelemetry();
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_tap_to_feedback",
      expect.objectContaining({
        control: "drop",
        feedback_kind: "confirm",
        duration_ms: expect.any(Number),
        sequence: expect.any(Number),
      }),
    );
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_tap_to_operation_complete",
      expect.objectContaining({
        control: "drop",
        op: "drop",
        ok: true,
        duration_ms: expect.any(Number),
      }),
    );
  });

  it("filters sensitive properties before persistence and capture", async () => {
    const backing = new Map();
    const store = createMemoryTelemetryStore(backing);
    setTelemetryStoreForTests(store);
    mockedPosthog.capture.mockImplementation(() => {
      throw new Error("hold queue");
    });
    initTelemetry();
    captureEvent("ajax_tap_to_feedback", {
      control: "run",
      terminal_output: "should drop",
    });
    await vi.waitFor(async () => {
      expect(await store.countPending()).toBe(1);
    });
    const stored = [...backing.values()][0];
    expect(stored?.properties).toEqual(
      expect.objectContaining({ control: "run" }),
    );
    expect(stored?.properties).not.toHaveProperty("terminal_output");
    if (stored) {
      stored.next_attempt_at = 0;
    }
    mockedPosthog.capture.mockReset();
    await flushTelemetryQueue(store, (event, props) => {
      mockedPosthog.capture(event, props);
    });
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_tap_to_feedback",
      expect.objectContaining({ control: "run" }),
    );
    expect(mockedPosthog.capture).not.toHaveBeenCalledWith(
      "ajax_tap_to_feedback",
      expect.objectContaining({ terminal_output: expect.anything() }),
    );
  });

  it("does not let caller props override event context", async () => {
    initTelemetry();
    track("ajax_swipe", {
      direction: "left",
      sequence: 999999,
      event_id: "caller-override",
      standalone: true,
    });
    await drainTelemetry();
    const props = mockedPosthog.capture.mock.calls[0]?.[1] as Record<
      string,
      unknown
    >;
    expect(props.event_id).not.toBe("caller-override");
    expect(props.sequence).not.toBe(999999);
    expect(typeof props.standalone).toBe("boolean");
  });

  it("does not queue events when telemetry is off", async () => {
    const store = createMemoryTelemetryStore();
    setTelemetryStoreForTests(store);
    vi.stubEnv("VITE_POSTHOG_KEY", "off");
    initTelemetry();
    track("ajax_swipe", { direction: "left" });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(await store.countPending()).toBe(0);
    expect(mockedPosthog.capture).not.toHaveBeenCalled();
  });
});

describe("captureSwipe", () => {
  it("records completed swipe metrics", async () => {
    initTelemetry();
    captureSwipe({
      direction: "left",
      duration_ms: 180,
      distance_px: 120,
      velocity_px_per_ms: 0.667,
      completed: true,
      cancelled: false,
      settle_ms: 220,
      from_route: "#/task/foo",
      to_route: "#/task/foo/diff",
    });
    await drainTelemetry();
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_swipe",
      expect.objectContaining({
        direction: "left",
        duration_ms: 180,
        distance_px: 120,
        velocity_px_per_ms: 0.667,
        completed: true,
        cancelled: false,
        settle_ms: 220,
        from_route: "#/task/foo",
        to_route: "#/task/foo/diff",
      }),
    );
  });

  it("records cancelled snap-back swipes", async () => {
    initTelemetry();
    captureSwipe({
      direction: "right",
      duration_ms: 90,
      distance_px: 40,
      velocity_px_per_ms: 0.444,
      completed: false,
      cancelled: true,
      settle_ms: 215,
    });
    await drainTelemetry();
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_swipe",
      expect.objectContaining({
        completed: false,
        cancelled: true,
      }),
    );
  });
});

describe("route and PWA helpers", () => {
  it("captures route visible after navigation start", async () => {
    initTelemetry();
    markNavigationStart("#/");
    captureRouteVisible({ to_route: "#/task/h1" });
    await drainTelemetry();
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_route_visible",
      expect.objectContaining({
        from_route: "#/",
        to_route: "#/task/h1",
        duration_ms: expect.any(Number),
      }),
    );
    expect(isNavigationPending()).toBe(false);
  });

  it("captures PWA launch once per boot", async () => {
    initTelemetry();
    capturePwaLaunch(500);
    capturePwaLaunch(900);
    await drainTelemetry();
    expect(mockedPosthog.capture).toHaveBeenCalledTimes(1);
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_pwa_launch",
      expect.objectContaining({ duration_ms: 500 }),
    );
  });

  it("captures PWA resume with hidden duration", async () => {
    initTelemetry();
    capturePwaResume({ duration_ms: 1200 });
    await drainTelemetry();
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_pwa_resume",
      expect.objectContaining({ duration_ms: 1200 }),
    );
  });

  it("emits telemetry diagnostic with queue status", async () => {
    initTelemetry();
    captureTelemetryDiagnostic();
    await drainTelemetry();
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_telemetry_diagnostic",
      expect.objectContaining({
        initialized: true,
        pending: expect.any(Number),
        standalone: expect.any(Boolean),
      }),
    );
  });
});

describe("durable queue integration", () => {
  it("flushes pending events on initTelemetry success", async () => {
    const backing = new Map();
    const store = createMemoryTelemetryStore(backing);
    setTelemetryStoreForTests(store);
    resetTelemetryForTests();
    resetTelemetryContextForTests();
    setTelemetryStoreForTests(createMemoryTelemetryStore(backing));
    const context = {
      event_id: "queued-before-init",
      session_id: "sess",
      install_id: "install",
      sequence: 1,
      route: "#/tasks",
      viewport_w: 100,
      viewport_h: 200,
      standalone: false,
    };
    await store.put({
      event_id: context.event_id,
      event_name: "ajax_swipe",
      properties: { ...context, direction: "right" },
      created_at: Date.now(),
      attempts: 0,
      next_attempt_at: 0,
    });

    initTelemetry();
    await drainTelemetry();

    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_swipe",
      expect.objectContaining({ direction: "right", event_id: "queued-before-init" }),
    );
    expect(await store.countPending()).toBe(0);
  });

  it("exposes queue status for diagnostics", async () => {
    const store = createMemoryTelemetryStore();
    setTelemetryStoreForTests(store);
    mockedPosthog.capture.mockImplementation(() => {
      throw new Error("hold queue");
    });
    expect(await getTelemetryQueueStatus()).toEqual({
      pending: 0,
      initialized: false,
    });

    initTelemetry();
    track("ajax_swipe", { direction: "left" });
    await vi.waitFor(async () => {
      expect(await getTelemetryQueueStatus()).toEqual({
        pending: 1,
        initialized: true,
      });
    });

    mockedPosthog.capture.mockReset();
    await drainTelemetry();
    expect(await getTelemetryQueueStatus()).toEqual({
      pending: 0,
      initialized: true,
    });
  });
});
