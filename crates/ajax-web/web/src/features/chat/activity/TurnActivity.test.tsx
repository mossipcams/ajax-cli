import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import TurnActivity from "./TurnActivity";
import type { ConversationItem, ToolCall } from "../session/public";

const call = (overrides: Partial<ToolCall> = {}): ToolCall => ({
  callId: "c1",
  title: "Read",
  kind: "read",
  status: "completed",
  locations: ["/repo/a.ts"],
  content: [],
  ...overrides,
});

const toolItem = (id: string, overrides: Partial<ToolCall> = {}): ConversationItem => ({
  kind: "tool",
  id,
  call: call({ callId: id, ...overrides }),
});

describe("TurnActivity", () => {
  it("renders nothing when a turn has no work items", () => {
    const { container } = render(<TurnActivity items={[]} live={false} attention={false} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("shows a summary row when work items exist", () => {
    render(<TurnActivity items={[toolItem("t1")]} live={false} attention={false} />);
    expect(screen.getByTestId("session-turn-work-summary")).toBeInTheDocument();
  });

  it("lists tool rows when collapsed", () => {
    render(
      <TurnActivity
        items={[toolItem("t1"), toolItem("t2", { kind: "edit", title: "Edit", locations: ["/repo/b.ts"] })]}
        live={false}
        attention={false}
      />,
    );

    expect(screen.getByTestId("session-turn-work")).toHaveAttribute("data-expanded", "false");
    expect(screen.getAllByTestId("session-tool-card")).toHaveLength(2);
  });

  it("shows the counted summary on a live collapsed turn when tool rows exist", () => {
    render(
      <TurnActivity
        items={[
          toolItem("t1"),
          toolItem("t2", {
            title: "cargo test",
            kind: "execute",
            status: "in_progress",
            locations: [],
          }),
        ]}
        live
        attention={false}
      />,
    );

    expect(screen.getByTestId("session-turn-work-summary")).toHaveTextContent(
      "Read 1 file · ran 1 command",
    );
    expect(screen.getAllByTestId("session-tool-card")).toHaveLength(2);
  });

  it("shows the current operation on a live collapsed turn with no tool rows yet", () => {
    render(
      <TurnActivity
        items={[{ kind: "thought", id: "t1", text: "Checking auth" }]}
        live
        attention={false}
      />,
    );

    expect(screen.getByTestId("session-turn-work-summary")).toHaveTextContent("Checking auth");
    expect(screen.queryByTestId("session-tool-card")).not.toBeInTheDocument();
  });
});
