import { describe, it, expect } from "vitest";
import { render, fireEvent, screen } from "@testing-library/react";
import Thought from "./Thought";

describe("ReasoningRow", () => {
  it("expands on tap to show the full reasoning text", () => {
    render(<Thought text="Checking auth middleware" />);
    expect(screen.queryByTestId("session-thinking-body")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /thinking/i }));
    expect(screen.getByTestId("session-thinking-body")).toHaveTextContent(
      "Checking auth middleware",
    );
  });
});
