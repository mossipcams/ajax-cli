import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen } from "@testing-library/react";
import MusterBar from "./MusterBar";

describe("MusterBar", () => {
  const segments = [
    { status: "error" as const, count: 1 },
    { status: "running" as const, count: 4 },
  ];

  it("renders nothing when the active fleet is empty", () => {
    render(<MusterBar segments={[]} selected={null} onSelect={vi.fn()} />);
    expect(screen.queryByRole("group", { name: "Fleet status" })).toBeNull();
  });

  it("renders one segment per active state with its count", () => {
    render(<MusterBar segments={segments} selected={null} onSelect={vi.fn()} />);
    expect(screen.getByRole("button", { name: /1 Error/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /4 Running/ })).toBeInTheDocument();
  });

  it("sizes segments proportionally to their count", () => {
    render(<MusterBar segments={segments} selected={null} onSelect={vi.fn()} />);
    const running = screen.getByRole("button", { name: /4 Running/ });
    expect(running.style.flexGrow).toBe("4");
  });

  it("selects a state on tap and clears it when the live one is tapped again", () => {
    const onSelect = vi.fn();
    const { rerender } = render(
      <MusterBar segments={segments} selected={null} onSelect={onSelect} />,
    );
    fireEvent.click(screen.getByRole("button", { name: /1 Error/ }));
    expect(onSelect).toHaveBeenCalledWith("error");

    rerender(<MusterBar segments={segments} selected="error" onSelect={onSelect} />);
    const selected = screen.getByRole("button", { name: /Showing 1 Error/ });
    expect(selected).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(selected);
    expect(onSelect).toHaveBeenCalledWith(null);
  });
});
