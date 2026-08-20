import { describe, it, expect, vi } from "vitest";
import { COCKPIT_RELOAD_PARAM, reloadCockpitDocument } from "./reloadCockpitDocument";

describe("reloadCockpitDocument", () => {
  it("replaces the shell document with a cache-busting query param and preserved hash (#1007)", () => {
    const replace = vi.fn();
    const location = {
      href: "https://ajax.local:8787/#/settings",
      replace,
    } as unknown as Location;

    vi.spyOn(Date, "now").mockReturnValue(1_700_000_000_000);
    reloadCockpitDocument(location);

    expect(replace).toHaveBeenCalledOnce();
    expect(replace).toHaveBeenCalledWith(
      `https://ajax.local:8787/?${COCKPIT_RELOAD_PARAM}=1700000000000#/settings`,
    );
    vi.restoreAllMocks();
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
    vi.restoreAllMocks();
  });
});
