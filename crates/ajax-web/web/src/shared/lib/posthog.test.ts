import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import posthog from "posthog-js";
import {
  POSTHOG_PROJECT_KEY,
  beginInteraction,
  captureEvent,
  endTapToFeedback,
  endTapToOperationComplete,
  initPostHog,
  isPostHogInitialized,
  posthogDistinctId,
  readAppVersion,
  resetPostHogForTests,
} from "./posthog";

vi.mock("posthog-js", () => ({
  default: {
    init: vi.fn(),
    identify: vi.fn(),
    capture: vi.fn(),
  },
}));

const mockedPosthog = vi.mocked(posthog);

beforeEach(() => {
  resetPostHogForTests();
  mockedPosthog.init.mockReset();
  mockedPosthog.identify.mockReset();
  mockedPosthog.capture.mockReset();
  document.head.innerHTML = "";
});

afterEach(() => {
  resetPostHogForTests();
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

describe("initPostHog", () => {
  it("initializes PostHog Cloud US with project defaults, vitals, and replay off", () => {
    document.head.innerHTML =
      '<meta name="ajax-app-version" content="9.8.7">';

    initPostHog();

    expect(mockedPosthog.init).toHaveBeenCalledTimes(1);
    expect(mockedPosthog.init).toHaveBeenCalledWith(POSTHOG_PROJECT_KEY, {
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
        web_vitals_allowed_metrics: ["LCP", "CLS", "FCP", "INP"],
      },
      disable_session_recording: true,
      capture_exceptions: false,
    });
    expect(isPostHogInitialized()).toBe(true);
  });

  it("identifies the operator surface with host metadata", () => {
    document.head.innerHTML =
      '<meta name="ajax-app-version" content="4.5.6">';

    initPostHog();

    expect(mockedPosthog.identify).toHaveBeenCalledWith(posthogDistinctId(), {
      host: window.location.hostname,
      origin: window.location.origin,
      app_version: "4.5.6",
    });
  });

  it("only initializes once", () => {
    initPostHog();
    initPostHog();

    expect(mockedPosthog.init).toHaveBeenCalledTimes(1);
    expect(mockedPosthog.identify).toHaveBeenCalledTimes(1);
  });

  it("fails soft when PostHog throws", () => {
    mockedPosthog.init.mockImplementation(() => {
      throw new Error("network down");
    });
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});

    expect(() => initPostHog()).not.toThrow();
    expect(warn).toHaveBeenCalled();
    expect(isPostHogInitialized()).toBe(false);

    warn.mockRestore();
  });
});

describe("captureEvent", () => {
  it("no-ops before init", () => {
    captureEvent("ajax_swipe", { direction: "left" });
    expect(mockedPosthog.capture).not.toHaveBeenCalled();
  });

  it("forwards to posthog.capture after init", () => {
    initPostHog();
    captureEvent("ajax_swipe", { direction: "left", duration_ms: 120 });
    expect(mockedPosthog.capture).toHaveBeenCalledWith("ajax_swipe", {
      direction: "left",
      duration_ms: 120,
    });
  });

  it("records tap-to-feedback and tap-to-operation-complete durations", () => {
    initPostHog();
    const id = beginInteraction("drop");
    endTapToFeedback(id, "confirm");
    endTapToOperationComplete(id, { ok: true, op: "drop" });

    expect(mockedPosthog.capture).toHaveBeenCalledWith(
      "ajax_tap_to_feedback",
      expect.objectContaining({
        control: "drop",
        feedback_kind: "confirm",
        duration_ms: expect.any(Number),
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
});
