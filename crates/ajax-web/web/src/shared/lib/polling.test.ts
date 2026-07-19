import { describe, it, expect } from "vitest";
import {
  REFRESH_INTERVAL_ACTIVE_MS,
  REFRESH_INTERVAL_TERMINAL_MS,
  REFRESH_INTERVAL_IDLE_MS,
  REFRESH_INTERVAL_HIDDEN_MS,
  VERSION_POLL_MS,
  VERSION_POLL_TERMINAL_MS,
  VERSION_POLL_HIDDEN_MS,
  RESTART_POLL_MS,
  cockpitRefreshIntervalMs,
  versionPollIntervalMs,
} from "./polling";

describe("cockpitRefreshIntervalMs", () => {
  it("cockpitRefreshIntervalMs returns hidden interval when document is not visible", () => {
    expect(
      cockpitRefreshIntervalMs({ visibilityState: "hidden", routeKind: "dashboard" }),
    ).toBe(REFRESH_INTERVAL_HIDDEN_MS);
    expect(
      cockpitRefreshIntervalMs({ visibilityState: "hidden", routeKind: "task" }),
    ).toBe(REFRESH_INTERVAL_HIDDEN_MS);
    expect(REFRESH_INTERVAL_HIDDEN_MS).toBe(60000);
  });

  it("cockpitRefreshIntervalMs returns terminal interval on task route when visible", () => {
    expect(
      cockpitRefreshIntervalMs({ visibilityState: "visible", routeKind: "task" }),
    ).toBe(REFRESH_INTERVAL_TERMINAL_MS);
    expect(REFRESH_INTERVAL_TERMINAL_MS).toBe(5000);
  });

  it("cockpitRefreshIntervalMs returns idle interval on settings route when visible", () => {
    expect(
      cockpitRefreshIntervalMs({ visibilityState: "visible", routeKind: "settings" }),
    ).toBe(REFRESH_INTERVAL_IDLE_MS);
    expect(REFRESH_INTERVAL_IDLE_MS).toBe(10000);
  });

  it("cockpitRefreshIntervalMs returns active interval on dashboard or project when visible", () => {
    expect(
      cockpitRefreshIntervalMs({ visibilityState: "visible", routeKind: "dashboard" }),
    ).toBe(REFRESH_INTERVAL_ACTIVE_MS);
    expect(
      cockpitRefreshIntervalMs({ visibilityState: "visible", routeKind: "project" }),
    ).toBe(REFRESH_INTERVAL_ACTIVE_MS);
    expect(REFRESH_INTERVAL_ACTIVE_MS).toBe(1000);
  });
});

describe("versionPollIntervalMs", () => {
  it("versionPollIntervalMs returns hidden / terminal / default intervals by context", () => {
    expect(
      versionPollIntervalMs({ visibilityState: "hidden", routeKind: "dashboard" }),
    ).toBe(VERSION_POLL_HIDDEN_MS);
    expect(
      versionPollIntervalMs({ visibilityState: "hidden", routeKind: "task" }),
    ).toBe(VERSION_POLL_HIDDEN_MS);
    expect(VERSION_POLL_HIDDEN_MS).toBe(300_000);

    expect(
      versionPollIntervalMs({ visibilityState: "visible", routeKind: "task" }),
    ).toBe(VERSION_POLL_TERMINAL_MS);
    expect(VERSION_POLL_TERMINAL_MS).toBe(120_000);

    expect(
      versionPollIntervalMs({ visibilityState: "visible", routeKind: "dashboard" }),
    ).toBe(VERSION_POLL_MS);
    expect(
      versionPollIntervalMs({ visibilityState: "visible", routeKind: "project" }),
    ).toBe(VERSION_POLL_MS);
    expect(
      versionPollIntervalMs({ visibilityState: "visible", routeKind: "settings" }),
    ).toBe(VERSION_POLL_MS);
    expect(VERSION_POLL_MS).toBe(30000);
  });
});

describe("restart poll", () => {
  it("restart poll constant stays at 500ms", () => {
    expect(RESTART_POLL_MS).toBe(500);
  });
});
