import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import Skeleton from "./Skeleton";

describe("Skeleton", () => {
  it("renders 4 skeleton rows by default with no data-testid on the root", () => {
    render(<Skeleton testid="skeleton-under-test" />);
    const root = screen.getByTestId("skeleton-under-test");
    expect(root).toHaveAttribute("aria-hidden", "true");
    expect(screen.getAllByTestId("skeleton-row")).toHaveLength(4);
  });

  it("omits root data-testid when none is provided", () => {
    render(<Skeleton />);
    expect(screen.queryByTestId("skeleton-under-test")).not.toBeInTheDocument();
    expect(screen.getAllByTestId("skeleton-row")).toHaveLength(4);
  });

  it("renders the requested number of rows", () => {
    render(<Skeleton rows={6} />);
    expect(screen.getAllByTestId("skeleton-row")).toHaveLength(6);
  });

  it("sets data-testid when testid is provided", () => {
    render(<Skeleton testid="task-skeleton" />);
    expect(screen.getByTestId("task-skeleton")).toBeInTheDocument();
  });
});
