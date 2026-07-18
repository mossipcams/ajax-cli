import { describe, it, expect } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useHashRoute } from "./useHashRoute";
import { parseRoute } from "../routes";

describe("useHashRoute", () => {
  it("returns the initial route from location.hash", () => {
    location.hash = "#/settings";
    const { result } = renderHook(() => useHashRoute());
    expect(result.current).toEqual(parseRoute(location.hash));
  });

  it("updates when hashchange fires", () => {
    location.hash = "#/";
    const { result } = renderHook(() => useHashRoute());
    expect(result.current).toEqual({ kind: "dashboard" });

    act(() => {
      location.hash = "#/settings";
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    });

    expect(result.current).toEqual({ kind: "settings" });
  });

  it("removes the hashchange listener on unmount", () => {
    location.hash = "#/";
    const { result, unmount } = renderHook(() => useHashRoute());

    act(() => {
      location.hash = "#/settings";
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    });
    expect(result.current).toEqual({ kind: "settings" });

    unmount();

    act(() => {
      location.hash = "#/";
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    });
    expect(result.current).toEqual({ kind: "settings" });
  });
});
