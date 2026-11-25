import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { useRouteFocus } from "./useRouteFocus";
import type { Route } from "@/shared/lib/routes";

function Harness({
  route,
  rows = [],
  transitioning = false,
  taskContent = null,
}: {
  route: Route;
  rows?: string[];
  transitioning?: boolean;
  taskContent?: ReactNode;
}) {
  useRouteFocus(route, transitioning);
  return (
    <>
      <section data-outlet="dashboard" data-testid="dashboard">
        {rows.map((handle) => (
          <div
            key={handle}
            className="task-row"
            role="button"
            tabIndex={0}
            data-handle={handle}
            data-testid={`row-${handle}`}
          />
        ))}
      </section>
      <section data-outlet="task" data-testid="task">
        {taskContent}
      </section>
    </>
  );
}

describe("useRouteFocus 1632", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("does not move focus on the initial mount", () => {
    const focus = vi.spyOn(HTMLElement.prototype, "focus");

    render(<Harness route={{ kind: "task", handle: "web/a" }} />);

    expect(focus).not.toHaveBeenCalled();
    expect(document.body).toHaveFocus();
  });

  it("focuses the new route outlet, not an input inside it", () => {
    const { rerender } = render(<Harness route={{ kind: "dashboard" }} taskContent={<input aria-label="message" />} />);
    const input = screen.getByRole("textbox", { name: "message" });
    const focusInput = vi.spyOn(input, "focus");

    rerender(<Harness route={{ kind: "task", handle: "web/a" }} taskContent={<input aria-label="message" />} />);

    expect(screen.getByTestId("task")).toHaveFocus();
    expect(screen.getByTestId("task")).toHaveAttribute("tabindex", "-1");
    expect(focusInput).not.toHaveBeenCalled();
    expect(input).not.toHaveFocus();
  });

  it("focuses the outlet with preventScroll", () => {
    const { rerender } = render(<Harness route={{ kind: "dashboard" }} />);
    const focus = vi.spyOn(screen.getByTestId("task"), "focus");

    rerender(<Harness route={{ kind: "task", handle: "web/a" }} />);

    expect(focus).toHaveBeenCalledWith({ preventScroll: true });
  });

  it("waits for a page transition to settle before moving focus", () => {
    const { rerender } = render(<Harness route={{ kind: "dashboard" }} />);

    rerender(<Harness route={{ kind: "task", handle: "web/a" }} transitioning />);
    expect(screen.getByTestId("task")).not.toHaveFocus();

    rerender(<Harness route={{ kind: "task", handle: "web/a" }} />);
    expect(screen.getByTestId("task")).toHaveFocus();
  });

  it("focuses the originating task row when returning to the dashboard", () => {
    const rows = ["web/a", "web/b"];
    const { rerender } = render(<Harness route={{ kind: "dashboard" }} rows={rows} />);
    rerender(<Harness route={{ kind: "task", handle: "web/a" }} rows={rows} />);
    rerender(<Harness route={{ kind: "dashboard" }} rows={rows} />);

    expect(screen.getByTestId("row-web/a")).toHaveFocus();
    expect(screen.getByTestId("row-web/b")).not.toHaveFocus();
  });

  it("focuses the originating row without scrolling", () => {
    const { rerender } = render(<Harness route={{ kind: "dashboard" }} rows={["web/a"]} />);
    rerender(<Harness route={{ kind: "task", handle: "web/a" }} rows={["web/a"]} />);
    const focus = vi.spyOn(screen.getByTestId("row-web/a"), "focus");

    rerender(<Harness route={{ kind: "dashboard" }} rows={["web/a"]} />);

    expect(focus).toHaveBeenCalledWith({ preventScroll: true });
  });

  it("falls back to the dashboard outlet when the originating row is gone", () => {
    const { rerender } = render(<Harness route={{ kind: "task", handle: "web/gone" }} />);

    rerender(<Harness route={{ kind: "dashboard" }} rows={["web/a"]} />);

    expect(screen.getByTestId("dashboard")).toHaveFocus();
  });

  it("does not steal focus when focus is already inside the new outlet", () => {
    const inside = <button type="button">inside</button>;
    const { rerender } = render(<Harness route={{ kind: "dashboard" }} taskContent={inside} />);
    screen.getByRole("button", { name: "inside" }).focus();
    const focus = vi.spyOn(screen.getByTestId("task"), "focus");

    rerender(<Harness route={{ kind: "task", handle: "web/a" }} taskContent={inside} />);

    expect(focus).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "inside" })).toHaveFocus();
  });
});
