import { describe, it, expect } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useHashRoute } from "./useHashRoute";
import { parseRoute } from "@/shared/lib/routes";

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

  // App.tsx has `useEffect(..., [route])` for the document title. A route object
  // rebuilt on every render would fire that effect on every render instead of
  // only on navigation, so identity stability is part of this hook's contract.
  it("keeps one route identity across re-renders while the hash is unchanged", () => {
    location.hash = "#/settings";
    const { result, rerender } = renderHook(() => useHashRoute());
    const first = result.current;

    rerender();
    rerender();

    expect(result.current).toBe(first);
  });

  it("returns a new route identity after the hash changes", () => {
    location.hash = "#/";
    const { result } = renderHook(() => useHashRoute());
    const first = result.current;

    act(() => {
      location.hash = "#/settings";
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    });

    expect(result.current).not.toBe(first);
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
