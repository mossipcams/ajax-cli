import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook } from "@testing-library/react";
import { useViewportBand } from "./useViewportBand";

const initViewport = vi.fn();

vi.mock("@/shared/lib/viewport", () => ({
  initViewport: () => initViewport(),
}));

describe("useViewportBand", () => {
  beforeEach(() => {
    initViewport.mockReset();
  });

  it("calls initViewport on mount and its cleanup on unmount", () => {
    const cleanup = vi.fn();
    initViewport.mockReturnValue(cleanup);

    const { unmount } = renderHook(() => useViewportBand());

    expect(initViewport).toHaveBeenCalledTimes(1);
    unmount();
    expect(cleanup).toHaveBeenCalledTimes(1);
  });
});
