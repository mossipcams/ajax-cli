import { describe, it, expect, beforeEach } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import RouteScroll, { readRouteScrollOffset } from "./RouteScroll";
import { parseRoute } from "@/shared/lib/routes";
import { useHashRoute } from "@/shared/hooks/useHashRoute";

function Harness({ restoreKey }: { restoreKey?: string | null }) {
  useHashRoute();
  return (
    <RouteScroll restoreKey={restoreKey}>
      <div>{window.location.hash}</div>
    </RouteScroll>
  );
}

function setScrollTop(node: HTMLElement, value: number) {
  Object.defineProperty(node, "scrollTop", { configurable: true, writable: true, value });
}

function clampScrollTop(node: HTMLElement, max: () => number) {
  let top = 0;
  Object.defineProperty(node, "scrollTop", {
    configurable: true,
    get: () => top,
    set: (value: number) => {
      top = Math.max(0, Math.min(value, max()));
    },
  });
}

function changeHash(hash: string) {
  act(() => {
    location.hash = hash;
    window.dispatchEvent(new HashChangeEvent("hashchange"));
  });
}

function wait(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

describe("RouteScroll", () => {
  beforeEach(() => {
    location.hash = "#/";
  });

  it("restores the scroll offset of a route when it is shown again", () => {
    render(<Harness />);
    const scroll = screen.getByTestId("route-scroll");
    setScrollTop(scroll, 0);

    setScrollTop(scroll, 1200);
    fireEvent.scroll(scroll);
    changeHash("#/t/a");
    expect(scroll.scrollTop).toBe(0);

    changeHash("#/");
    expect(scroll.scrollTop).toBe(1200);
  });

  it("does not let a clamp-style scroll during the swap overwrite the saved offset", () => {
    render(<Harness />);
    const scroll = screen.getByTestId("route-scroll");
    setScrollTop(scroll, 0);

    setScrollTop(scroll, 1200);
    fireEvent.scroll(scroll);
    changeHash("#/t/a");
    setScrollTop(scroll, 0);
    fireEvent.scroll(scroll);

    changeHash("#/");
    expect(scroll.scrollTop).toBe(1200);
  });

  it("keys the dashboard by its parsed route so the bare URL and #/ share an offset", () => {
    history.replaceState(null, "", "/");
    render(<Harness />);
    const scroll = screen.getByTestId("route-scroll");
    setScrollTop(scroll, 1200);
    fireEvent.scroll(scroll);

    changeHash("#/t/a");
    changeHash("#/");
    expect(scroll.scrollTop).toBe(1200);
  });

  it("re-applies a restore when the destination content grows after the first commit", async () => {
    let max = 5000;
    render(<Harness />);
    const scroll = screen.getByTestId("route-scroll");
    clampScrollTop(scroll, () => max);

    scroll.scrollTop = 1200;
    fireEvent.scroll(scroll);
    changeHash("#/t/a");

    max = 0;
    changeHash("#/");
    expect(scroll.scrollTop).toBe(0);
    fireEvent.scroll(scroll);

    max = 5000;
    await wait(120);
    expect(scroll.scrollTop).toBe(1200);

    changeHash("#/t/a");
    changeHash("#/");
    expect(scroll.scrollTop).toBe(1200);
  });

  it("restores the offset in the commit that swaps in the real content, before paint", () => {
    let max = 5000;
    const { rerender } = render(<Harness />);
    const scroll = screen.getByTestId("route-scroll");
    clampScrollTop(scroll, () => max);

    scroll.scrollTop = 300;
    fireEvent.scroll(scroll);
    changeHash("#/t/a");

    max = 0;
    changeHash("#/");
    expect(scroll.scrollTop).toBe(0);

    max = 5000;
    rerender(<Harness restoreKey="stack" />);
    expect(scroll.scrollTop).toBe(300);
  });

  it("exposes the saved offset through readRouteScrollOffset and clears it on unmount", () => {
    const { unmount } = render(<Harness />);
    const scroll = screen.getByTestId("route-scroll");
    setScrollTop(scroll, 0);

    setScrollTop(scroll, 1200);
    fireEvent.scroll(scroll);
    changeHash("#/t/a");

    expect(readRouteScrollOffset(parseRoute("#/"))).toBe(1200);
    expect(readRouteScrollOffset(parseRoute("#/t/zzz"))).toBe(0);

    unmount();
    expect(readRouteScrollOffset(parseRoute("#/"))).toBe(0);
  });
});
