import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  buildEventContext,
  getInstallId,
  getSessionId,
  isStandaloneDisplay,
  nextSequence,
  resetTelemetryContextForTests,
} from "./telemetryContext";

function stubMatchMedia(matches: boolean) {
  vi.stubGlobal(
    "matchMedia",
    vi.fn().mockImplementation((query: string) => ({
      matches: query === "(display-mode: standalone)" ? matches : false,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  );
}

beforeEach(() => {
  resetTelemetryContextForTests();
  localStorage.clear();
  sessionStorage.clear();
  vi.stubGlobal("innerWidth", 390);
  vi.stubGlobal("innerHeight", 844);
  vi.stubGlobal("devicePixelRatio", 2);
  Object.defineProperty(navigator, "onLine", {
    configurable: true,
    value: true,
  });
  window.location.hash = "#/";
  document.head.innerHTML = "";
});

afterEach(() => {
  resetTelemetryContextForTests();
  localStorage.clear();
  sessionStorage.clear();
  vi.unstubAllGlobals();
});

describe("isStandaloneDisplay", () => {
  it("returns true when display-mode standalone matches", () => {
    stubMatchMedia(true);
    expect(isStandaloneDisplay()).toBe(true);
  });

  it("returns true when navigator.standalone is set (iOS)", () => {
    stubMatchMedia(false);
    Object.defineProperty(navigator, "standalone", {
      configurable: true,
      value: true,
    });
    expect(isStandaloneDisplay()).toBe(true);
    Object.defineProperty(navigator, "standalone", {
      configurable: true,
      value: undefined,
    });
  });

  it("returns false in a normal browser tab", () => {
    stubMatchMedia(false);
    expect(isStandaloneDisplay()).toBe(false);
  });
});

describe("nextSequence", () => {
  it("increments monotonically per install id", () => {
    const installId = getInstallId();
    expect(nextSequence()).toBe(1);
    expect(nextSequence()).toBe(2);
    expect(nextSequence()).toBe(3);
    expect(getInstallId()).toBe(installId);
  });

  it("resets when install id changes", () => {
    nextSequence();
    nextSequence();
    resetTelemetryContextForTests();
    localStorage.setItem("ajax:telemetry:install_id", "other-install");
    expect(nextSequence()).toBe(1);
  });
});

describe("getSessionId", () => {
  it("persists in sessionStorage, not localStorage", () => {
    const first = getSessionId();
    expect(getSessionId()).toBe(first);
    expect(sessionStorage.getItem("ajax:telemetry:session_id")).toBe(first);
    expect(localStorage.getItem("ajax:telemetry:session_id")).toBeNull();
  });
});

describe("buildEventContext", () => {
  it("includes required context fields", () => {
    document.head.innerHTML =
      '<meta name="ajax-app-version" content="1.2.3-test">';
    window.location.hash = "#/p/demo";
    stubMatchMedia(false);

    const ctx = buildEventContext();

    expect(ctx.event_id).toBeTruthy();
    expect(ctx.session_id).toBeTruthy();
    expect(ctx.install_id).toBeTruthy();
    expect(ctx.sequence).toBe(1);
    expect(ctx.app_version).toBe("1.2.3-test");
    expect(ctx.route).toBe("#/p/demo");
    expect(ctx.route_kind).toBe("project");
    expect(ctx.host).toBe(window.location.hostname);
    expect(ctx.online).toBe(true);
    expect(ctx.visibility).toBe(document.visibilityState);
    expect(ctx.pixel_ratio).toBe(2);
    expect(ctx.viewport_w).toBe(390);
    expect(ctx.viewport_h).toBe(844);
    expect(ctx.standalone).toBe(false);
  });
});
