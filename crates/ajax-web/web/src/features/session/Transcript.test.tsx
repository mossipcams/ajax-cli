import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import Transcript from "./Transcript";
import type { ThreadEntry } from "./sessionThread";

const agentProse = (id: string, text: string): ThreadEntry => ({
  kind: "prose",
  id,
  role: "agent",
  text,
});

describe("Transcript", () => {
  it("renders the live agent tail as markdown while busy", () => {
    const entries = [agentProse("e1", "Still **streaming**")];
    render(<Transcript entries={entries} busy />);
    const message = screen.getByTestId("session-message-agent");
    expect(message).toHaveAttribute("data-live", "true");
    expect(message).toHaveClass("is-live");
    expect(message).toHaveTextContent("Still streaming");
    expect(screen.queryByRole("list")).not.toBeInTheDocument();
    expect(screen.getByText("streaming").tagName).toBe("STRONG");
  });

  it("renders settled agent prose as markdown after the turn ends", () => {
    const entries = [agentProse("e1", "Done:\n\n- item")];
    render(<Transcript entries={entries} busy={false} />);
    const message = screen.getByTestId("session-message-agent");
    expect(message).not.toHaveAttribute("data-live");
    expect(screen.getByRole("listitem")).toHaveTextContent("item");
  });

  it("keeps earlier agent prose on markdown when a new tail streams", () => {
    const entries = [
      agentProse("e1", "First:\n\n- done"),
      agentProse("e2", "Next **chunk**"),
    ];
    render(<Transcript entries={entries} busy />);
    const messages = screen.getAllByTestId("session-message-agent");
    expect(messages[0]).not.toHaveAttribute("data-live");
    expect(screen.getByRole("listitem")).toHaveTextContent("done");
    expect(messages[1]).toHaveAttribute("data-live", "true");
    expect(messages[1]).toHaveTextContent("Next chunk");
    expect(screen.getByText("chunk").tagName).toBe("STRONG");
    expect(screen.getAllByRole("listitem")).toHaveLength(1);
  });
});
