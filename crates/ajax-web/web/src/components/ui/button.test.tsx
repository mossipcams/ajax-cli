import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { Button } from "./button";

describe("Button", () => {
  it("renders a native button with data-slot", () => {
    render(<Button>Click</Button>);
    const button = screen.getByRole("button", { name: "Click" });
    expect(button.tagName).toBe("BUTTON");
    expect(button).toHaveAttribute("data-slot", "button");
  });

  it("reflects data-variant and data-size from props", () => {
    render(
      <Button variant="secondary" size="default">
        Label
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Label" });
    expect(button).toHaveAttribute("data-variant", "secondary");
    expect(button).toHaveAttribute("data-size", "default");
  });

  it.each([
    ["default", "pill is-primary"],
    ["secondary", "pill"],
    ["destructive", "pill is-danger"],
  ] as const)("variant %s emits Ajax classes %s", (variant, expectedClasses) => {
    render(<Button variant={variant}>Action</Button>);
    const button = screen.getByRole("button", { name: "Action" });
    for (const className of expectedClasses.split(" ")) {
      expect(button.classList.contains(className)).toBe(true);
    }
  });

  it("merges className with variant classes", () => {
    render(
      <Button variant="secondary" className="settings-back">
        Back
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Back" });
    expect(button.classList.contains("pill")).toBe(true);
    expect(button.classList.contains("settings-back")).toBe(true);
  });

  it("passes through native button props", () => {
    const onClick = vi.fn();
    const { rerender } = render(
      <Button type="submit" disabled aria-label="Start task" onClick={onClick}>
        Start
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Start task" });
    expect(button).toHaveAttribute("type", "submit");
    expect(button).toBeDisabled();

    rerender(
      <Button type="submit" aria-label="Start task" onClick={onClick}>
        Start
      </Button>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Start task" }));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("renders asChild with merged classes on the child element", () => {
    render(
      <Button variant="default" asChild>
        <a href="/tasks">Tasks</a>
      </Button>,
    );
    const link = screen.getByRole("link", { name: "Tasks" });
    expect(link.tagName).toBe("A");
    expect(link).toHaveAttribute("data-slot", "button");
    expect(link.classList.contains("pill")).toBe(true);
    expect(link.classList.contains("is-primary")).toBe(true);
  });
});
