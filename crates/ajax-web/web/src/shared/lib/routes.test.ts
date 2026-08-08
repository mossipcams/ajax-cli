import { describe, it, expect } from "vitest";
import {
  parseRoute,
  dashboardHash,
  projectHash,
  taskHash,
  taskDiffHash,
  settingsHash,
  sessionHash,
} from "./routes";

describe("parseRoute", () => {
  it("treats empty hash as dashboard", () => {
    expect(parseRoute("")).toEqual({ kind: "dashboard" });
    expect(parseRoute("#/")).toEqual({ kind: "dashboard" });
  });

  it("parses the settings route", () => {
    expect(parseRoute("#/settings")).toEqual({ kind: "settings" });
  });

  it("parses session starter and orchestration routes", () => {
    expect(parseRoute("#/session")).toEqual({ kind: "session" });
    expect(parseRoute("#/session/web%2Ffix-login")).toEqual({
      kind: "session",
      handle: "web/fix-login",
    });
    expect(parseRoute("#/session/")).toEqual({ kind: "session" });
  });

  it("parses a project route and decodes the name", () => {
    expect(parseRoute("#/p/web")).toEqual({ kind: "project", project: "web" });
    expect(parseRoute("#/p/my%20repo")).toEqual({ kind: "project", project: "my repo" });
  });

  it("treats an empty project as the dashboard", () => {
    expect(parseRoute("#/p/")).toEqual({ kind: "dashboard" });
  });

  it("parses a task route and decodes the handle", () => {
    expect(parseRoute("#/t/web%2Ffix-login")).toEqual({
      kind: "task",
      handle: "web/fix-login",
    });
  });

  it("parses a task diff route and optional pr query", () => {
    expect(parseRoute("#/t/web%2Ffix-login/diff")).toEqual({
      kind: "diff",
      handle: "web/fix-login",
    });
    expect(parseRoute("#/t/web%2Ffix-login/diff?pr=12")).toEqual({
      kind: "diff",
      handle: "web/fix-login",
      pr: 12,
    });
  });

  it("falls back to dashboard for unknown hashes", () => {
    expect(parseRoute("#/garbage")).toEqual({ kind: "dashboard" });
  });
});

describe("route formatters", () => {
  it("formats hashes with encoding", () => {
    expect(dashboardHash()).toBe("#/");
    expect(settingsHash()).toBe("#/settings");
    expect(projectHash("my repo")).toBe("#/p/my%20repo");
    expect(taskHash("web/fix-login")).toBe("#/t/web%2Ffix-login");
    expect(taskDiffHash("web/fix-login")).toBe("#/t/web%2Ffix-login/diff");
    expect(taskDiffHash("web/fix-login", 12)).toBe("#/t/web%2Ffix-login/diff?pr=12");
    expect(sessionHash()).toBe("#/session");
    expect(sessionHash("web/fix-login")).toBe("#/session/web%2Ffix-login");
  });
});
