import { afterEach, describe, it, expect, vi } from "vitest";
import {
  COCKPIT_RELOAD_PARAM,
  COCKPIT_RELOAD_WATCH_MS,
  reloadCockpitDocument,
} from "./reloadCockpitDocument";

describe("reloadCockpitDocument", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("replaces the shell document with a cache-busting query param and preserved hash (#1007)", () => {
    const replace = vi.fn();
    const location = {
      href: "https://ajax.local:8787/#/settings",
      replace,
    } as unknown as Location;

    vi.spyOn(Date, "now").mockReturnValue(1_700_000_000_000);
    expect(reloadCockpitDocument(location)).toBe(true);

    expect(replace).toHaveBeenCalledExactlyOnceWith(
      `https://ajax.local:8787/?${COCKPIT_RELOAD_PARAM}=1700000000000#/settings`,
    );
  });

  it("updates an existing reload param so repeated navigations stay distinct", () => {
    const replace = vi.fn();
    const location = {
      href: `https://ajax.local:8787/?${COCKPIT_RELOAD_PARAM}=1#/`,
      replace,
    } as unknown as Location;

    vi.spyOn(Date, "now").mockReturnValue(2);
    reloadCockpitDocument(location);

    expect(replace).toHaveBeenCalledWith(`https://ajax.local:8787/?${COCKPIT_RELOAD_PARAM}=2#/`);
  });

  it("returns false when location.replace throws", () => {
    const replace = vi.fn(() => {
      throw new Error("blocked");
    });
    const location = {
      href: "https://ajax.local:8787/#/",
      replace,
    } as unknown as Location;

    expect(reloadCockpitDocument(location)).toBe(false);
  });

  it("calls onNavigationMissed when the document URL is unchanged after the watch interval (#1007)", () => {
    vi.useFakeTimers();
    const replace = vi.fn();
    const location = {
      href: "https://ajax.local:8787/#/",
      replace,
    } as unknown as Location;
    const onNavigationMissed = vi.fn();

    vi.spyOn(Date, "now").mockReturnValue(9);
    expect(
      reloadCockpitDocument(location, {
        onNavigationMissed,
      }),
    ).toBe(true);
    expect(onNavigationMissed).not.toHaveBeenCalled();

    vi.advanceTimersByTime(COCKPIT_RELOAD_WATCH_MS - 1);
    expect(onNavigationMissed).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(onNavigationMissed).toHaveBeenCalledOnce();
  });
});
