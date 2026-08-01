import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  setSwipeEnterDirection,
  consumeSwipeEnterDirection,
  navigateHashWithEnter,
  swipeEnterClassName,
} from "./swipeEnter";

describe("swipeEnter", () => {
  beforeEach(() => {
    sessionStorage.clear();
  });

  afterEach(() => {
    sessionStorage.clear();
  });

  it("stores and consumes a direction once", () => {
    setSwipeEnterDirection("left");
    expect(consumeSwipeEnterDirection()).toBe("left");
    expect(consumeSwipeEnterDirection()).toBeNull();
  });

  it("maps direction to outlet enter class", () => {
    expect(swipeEnterClassName("left")).toBe("ajax-swipe-enter-left");
    expect(swipeEnterClassName(null)).toBe("");
  });

  it("stores enter direction before assigning hash", () => {
    navigateHashWithEnter("#/t/web%2Ffix-login", "left");
    expect(sessionStorage.getItem("ajax-swipe-enter")).toBe("left");
    expect(window.location.hash).toBe("#/t/web%2Ffix-login");
  });
});
