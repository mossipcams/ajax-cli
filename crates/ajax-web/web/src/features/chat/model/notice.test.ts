import { describe, it, expect } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useSessionModelNotice, useSessionModelSheet } from "./notice";

describe("useSessionModelNotice", () => {
  it("shows and clears dismissable notices", () => {
    const { result } = renderHook(() => useSessionModelNotice());
    expect(result.current.notice).toBeNull();
    act(() => result.current.showNotice("refused"));
    expect(result.current.notice).toBe("refused");
    act(() => result.current.dismissNotice());
    expect(result.current.notice).toBeNull();
  });
});

describe("useSessionModelSheet", () => {
  it("tracks model sheet open state", () => {
    const { result } = renderHook(() => useSessionModelSheet());
    expect(result.current.modelSheetOpen).toBe(false);
    act(() => result.current.setModelSheetOpen(true));
    expect(result.current.modelSheetOpen).toBe(true);
  });
});
