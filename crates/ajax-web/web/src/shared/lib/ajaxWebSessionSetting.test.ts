import { describe, it, expect, afterEach, vi } from "vitest";
import {
  isAjaxWebSessionEnabled,
  setAjaxWebSessionEnabled,
} from "./ajaxWebSessionSetting";

afterEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

describe("ajaxWebSessionSetting", () => {
  it("defaults to disabled when unset", () => {
    expect(isAjaxWebSessionEnabled()).toBe(false);
  });

  it("persists enabled state as true/false strings", () => {
    setAjaxWebSessionEnabled(true);
    expect(localStorage.getItem("ajax.webSession")).toBe("true");
    expect(isAjaxWebSessionEnabled()).toBe(true);

    setAjaxWebSessionEnabled(false);
    expect(localStorage.getItem("ajax.webSession")).toBe("false");
    expect(isAjaxWebSessionEnabled()).toBe(false);
  });

  it("returns false when localStorage read throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("private mode");
    });
    expect(isAjaxWebSessionEnabled()).toBe(false);
  });

  it("swallows localStorage write errors", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("private mode");
    });
    expect(() => setAjaxWebSessionEnabled(true)).not.toThrow();
    expect(isAjaxWebSessionEnabled()).toBe(false);
  });
});
