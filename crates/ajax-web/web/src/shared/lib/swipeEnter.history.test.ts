import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { navigateHashWithEnter, navigateHashBack } from "./swipeEnter";

describe("swipeEnter history", () => {
  beforeEach(() => {
    sessionStorage.clear();
    history.replaceState(null, "", "/");
  });

  afterEach(() => {
    vi.restoreAllMocks();
    sessionStorage.clear();
  });

  it("records the previous hash on the new history entry", () => {
    history.replaceState(null, "", "/#/");
    navigateHashWithEnter("#/t/web%2Ffix-login", "left");
    expect(location.hash).toBe("#/t/web%2Ffix-login");
    expect(history.state).toEqual({ ajaxPrevHash: "#/" });
  });

  it("calls history.back when going back to the recorded hash", () => {
    history.replaceState(null, "", "/#/");
    navigateHashWithEnter("#/t/web%2Ffix-login", "left");
    const back = vi.spyOn(history, "back").mockImplementation(() => {});
    navigateHashBack("#/");
    expect(back).toHaveBeenCalledOnce();
    expect(sessionStorage.getItem("ajax-swipe-enter")).toBe("right");
  });

  it("replaces the current entry when going back to a different hash", () => {
    history.replaceState(null, "", "/#/");
    navigateHashWithEnter("#/t/a", "left");
    navigateHashWithEnter("#/t/b", "left");
    const lengthBefore = history.length;
    const back = vi.spyOn(history, "back");
    navigateHashBack("#/");
    expect(back).not.toHaveBeenCalled();
    expect(location.hash).toBe("#/");
    expect(history.length).toBe(lengthBefore);
  });
});
