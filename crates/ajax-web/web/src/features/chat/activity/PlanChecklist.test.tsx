import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import PlanChecklist from "./PlanChecklist";

describe("PlanChecklist", () => {
  it("renders plan steps with status markers", () => {
    render(
      <PlanChecklist
        entries={[
          { content: "Read", status: "completed" },
          { content: "Patch", status: "in_progress" },
        ]}
      />,
    );
    const steps = screen.getAllByRole("listitem");
    expect(steps).toHaveLength(2);
    expect(steps[1]).toHaveAttribute("data-status", "in_progress");
  });
});
