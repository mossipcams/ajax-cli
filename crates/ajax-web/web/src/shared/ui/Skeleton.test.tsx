import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import Skeleton from "./Skeleton";

describe("Skeleton", () => {
  it("renders 4 skeleton rows by default with no data-testid", () => {
    const { container } = render(<Skeleton />);
    const root = container.querySelector(".skeleton");
    expect(root).not.toBeNull();
    expect(root?.getAttribute("data-testid")).toBeNull();
    expect(root?.getAttribute("aria-hidden")).toBe("true");
    expect(container.querySelectorAll(".skeleton-row")).toHaveLength(4);
  });

  it("renders the requested number of rows", () => {
    const { container } = render(<Skeleton rows={6} />);
    expect(container.querySelectorAll(".skeleton-row")).toHaveLength(6);
  });

  it("sets data-testid when testid is provided", () => {
    render(<Skeleton testid="task-skeleton" />);
    expect(screen.getByTestId("task-skeleton").getAttribute("data-testid")).toBe(
      "task-skeleton",
    );
  });
});
