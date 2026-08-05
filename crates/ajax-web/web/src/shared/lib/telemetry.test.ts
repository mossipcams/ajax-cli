import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import posthog from "posthog-js";
import { resetTelemetryContextForTests } from "./telemetryContext";
import {
  DEFAULT_POSTHOG_PROJECT_KEY,
  beginInteraction,
  captureEvent,
  capturePwaLaunch,
  capturePwaResume,
  captureRouteVisible,
  captureSwipe,
  captureTelemetryDiagnostic,
  endTapToFeedback,
  endTapToOperationComplete,
  getTelemetryStatus,
  initTelemetry,
  isNavigationPending,
  isTelemetryInitialized,
  markNavigationStart,
  readAppVersion,
  resetTelemetryForTests,
  telemetryDistinctId,
  track,
} from "./telemetry";

vi.mock("posthog-js", () => ({
  default: {
    init: vi.fn(),
    identify: vi.fn(),
    register: vi.fn(),
    capture: vi.fn(),
  },
}));

const mockedPosthog = vi.mocked(posthog);

beforeEach(() => {
  resetTelemetryForTests();
  resetTelemetryContextForTests();
  localStorage.clear();
  sessionStorage.clear();
  mockedPosthog.init.mockReset();
  mockedPosthog.identify.mockReset();
  mockedPosthog.register.mockReset();
  mockedPosthog.capture.mockReset();
  document.head.innerHTML = "";
  vi.stubEnv("VITE_POSTHOG_KEY", "phc_test_key");
  vi.stubEnv("VITE_POSTHOG_HOST", "");
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

  it("initializes PostHog Cloud with env key, web vitals, and replay off", () => {
    document.head.innerHTML =
      '<meta name="ajax-app-version" content="9.8.7">';
    initTelemetry();
    expect(mockedPosthog.init).toHaveBeenCalledWith("phc_test_key", {
      api_host: "https://us.i.posthog.com",
      defaults: "2026-05-30",
      autocapture: {
        css_selector_ignorelist: expect.arrayContaining([
          ".ph-no-autocapture",
          ".terminal-host",
        ]),
      },
      capture_performance: {
        web_vitals: true,
        web_vitals_allowed_metrics: ["LCP", "CLS", "FCP", "INP"],
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

  it("registers super-properties for automatic events", () => {
    document.head.innerHTML =
      '<meta name="ajax-app-version" content="4.5.6">';
    initTelemetry();
    expect(mockedPosthog.register).toHaveBeenCalledWith(
      expect.objectContaining({
        standalone: expect.any(Boolean),
        install_id: expect.any(String),
        host: window.location.hostname,
        app_version: "4.5.6",
      }),
    );
  });

  it("only initializes once", () => {
    initTelemetry();
    initTelemetry();
    expect(mockedPosthog.init).toHaveBeenCalledTimes(1);
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
  it("no-ops before init", () => {
    vi.stubEnv("VITE_POSTHOG_KEY", "off");
    initTelemetry();
    track("ajax_swipe", { direction: "left" });
    expect(mockedPosthog.capture).not.toHaveBeenCalled();
  });

  it("enriches events with context and forwards to posthog.capture", () => {
    initTelemetry();
    track("ajax_swipe", { direction: "left", duration_ms: 120 });
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
        route_kind: expect.any(String),
        host: expect.any(String),
        online: expect.any(Boolean),
        visibility: expect.any(String),
        pixel_ratio: expect.any(Number),
        viewport_w: expect.any(Number),
        viewport_h: expect.any(Number),
        standalone: expect.any(Boolean),
      }),
    );
  });

  it("records tap-to-feedback and tap-to-operation-complete durations", () => {
    initTelemetry();
    const id = beginInteraction("drop");
    endTapToFeedback(id, "confirm");
    endTapToOperationComplete(id, { ok: true, op: "drop" });
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_tap_to_feedback",
      expect.objectContaining({
        interaction_id: id,
        control: "drop",
        feedback_kind: "confirm",
        duration_ms: expect.any(Number),
      }),
    );
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_tap_to_operation_complete",
      expect.objectContaining({
        interaction_id: id,
        control: "drop",
        op: "drop",
        ok: true,
        outcome: "success",
        feedback_kind: "confirm",
        feedback_ms: expect.any(Number),
        duration_ms: expect.any(Number),
      }),
    );
  });

  it("classifies cancelled and failed operation outcomes", () => {
    initTelemetry();
    const cancelled = beginInteraction("drop");
    endTapToFeedback(cancelled, "confirm");
    endTapToOperationComplete(cancelled, {
      ok: false,
      op: "drop",
      error_kind: "undo",
    });
    const failed = beginInteraction("review");
    endTapToOperationComplete(failed, {
      ok: false,
      op: "review",
      error_kind: "network",
    });
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_tap_to_operation_complete",
      expect.objectContaining({
        control: "drop",
        outcome: "cancelled",
        error_kind: "undo",
      }),
    );
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_tap_to_operation_complete",
      expect.objectContaining({
        control: "review",
        outcome: "failed",
        error_kind: "network",
      }),
    );
  });

  it("filters sensitive properties before capture", () => {
    initTelemetry();
    captureEvent("ajax_tap_to_feedback", {
      control: "run",
      terminal_output: "should drop",
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

  it("does not let caller props override event context", () => {
    initTelemetry();
    track("ajax_swipe", {
      direction: "left",
      sequence: 999999,
      event_id: "caller-override",
      standalone: true,
    });
    const props = mockedPosthog.capture.mock.calls[0]?.[1] as Record<
      string,
      unknown
    >;
    expect(props.event_id).not.toBe("caller-override");
    expect(props.sequence).not.toBe(999999);
    expect(typeof props.standalone).toBe("boolean");
  });
});

describe("captureSwipe", () => {
  it("records completed and cancelled swipe metrics", () => {
    initTelemetry();
    captureSwipe({
      direction: "left",
      duration_ms: 180,
      distance_px: 120,
      page_width_px: 390,
      velocity_px_per_ms: 0.667,
      completed: true,
      cancelled: false,
      settle_ms: 220,
    });
    captureSwipe({
      direction: "right",
      duration_ms: 90,
      distance_px: 40,
      page_width_px: 390,
      velocity_px_per_ms: 0.444,
      completed: false,
      cancelled: true,
      settle_ms: 215,
    });
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_swipe",
      expect.objectContaining({
        completed: true,
        cancelled: false,
        page_width_px: 390,
        progress: 0.308,
        outcome: "completed",
      }),
    );
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_swipe",
      expect.objectContaining({
        completed: false,
        cancelled: true,
        page_width_px: 390,
        progress: 0.103,
        outcome: "cancelled",
      }),
    );
  });
});

describe("route and PWA helpers", () => {
  it("captures route visible after navigation start", () => {
    initTelemetry();
    markNavigationStart("#/", "hash");
    captureRouteVisible({ to_route: "#/t/h1" });
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_route_visible",
      expect.objectContaining({
        from_route: "#/",
        to_route: "#/t/h1",
        nav_trigger: "hash",
        route_kind: "task",
        duration_ms: expect.any(Number),
      }),
    );
    expect(isNavigationPending()).toBe(false);
  });

  it("markNavigationStart overwrites a pending swipe navigation mark", () => {
    initTelemetry();
    markNavigationStart("#/t/h1", "swipe");
    markNavigationStart(undefined, "hash");
    captureRouteVisible({ to_route: "#/settings" });
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_route_visible",
      expect.objectContaining({
        nav_trigger: "hash",
        to_route: "#/settings",
      }),
    );
  });

  it("captures PWA launch with navigation timing when available", () => {
    initTelemetry();
    const navEntry = {
      type: "navigate",
      domInteractive: 1234.7,
    } as PerformanceNavigationTiming;
    vi.spyOn(performance, "getEntriesByType").mockReturnValue([navEntry]);
    capturePwaLaunch(500);
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_pwa_launch",
      expect.objectContaining({
        duration_ms: 500,
        nav_type: "navigate",
        dom_interactive_ms: 1235,
      }),
    );
    vi.mocked(performance.getEntriesByType).mockRestore();
  });

  it("captures PWA launch once and resume timing", () => {
    initTelemetry();
    capturePwaLaunch(500);
    capturePwaLaunch(900);
    capturePwaResume({
      hidden_ms: 1200,
      resume_to_visible_ms: 40,
      resume_to_cockpit_ms: 900,
      resume_debounce_ms: 750,
      online: true,
      cockpit_ok: true,
    });
    expect(mockedPosthog.capture).toHaveBeenCalledTimes(2);
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_pwa_launch",
      expect.objectContaining({ duration_ms: 500 }),
    );
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_pwa_resume",
      expect.objectContaining({
        hidden_ms: 1200,
        resume_to_visible_ms: 40,
        resume_to_cockpit_ms: 900,
        resume_debounce_ms: 750,
        online: true,
        cockpit_ok: true,
      }),
    );
  });

  it("emits telemetry diagnostic with status and context fields", () => {
    initTelemetry();
    captureTelemetryDiagnostic();
    expect(getTelemetryStatus()).toEqual({
      initialized: true,
      standalone: expect.any(Boolean),
    });
    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_telemetry_diagnostic",
      expect.objectContaining({
        initialized: true,
        standalone: expect.any(Boolean),
        online: expect.any(Boolean),
        visibility: expect.any(String),
        route: expect.any(String),
        route_kind: expect.any(String),
        sequence: expect.any(Number),
        host: expect.any(String),
      }),
    );
  });
});
