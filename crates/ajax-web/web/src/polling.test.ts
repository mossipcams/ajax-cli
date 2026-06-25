import { describe, it, expect } from "vitest";
import { paneInterval, PANE_INTERVALS } from "./polling";

describe("paneInterval", () => {
  it("uses the idle cadence while the document is hidden", () => {
    expect(paneInterval({ hidden: true, stateKind: "AgentRunning" })).toBe(
      PANE_INTERVALS.idle,
    );
  });

  it("uses the default cadence with no state kind", () => {
    expect(paneInterval({ hidden: false, stateKind: undefined })).toBe(
      PANE_INTERVALS.default,
    );
  });

  it("uses the active cadence for live kinds", () => {
    for (const kind of ["WaitingForApproval", "WaitingForInput", "AgentRunning"]) {
      expect(paneInterval({ hidden: false, stateKind: kind })).toBe(
        PANE_INTERVALS.default,
      );
    }
  });

  it("uses the idle cadence for terminal kinds", () => {
    for (const kind of ["Done", "Idle"]) {
      expect(paneInterval({ hidden: false, stateKind: kind })).toBe(
        PANE_INTERVALS.idle,
      );
    }
  });

  it("uses the unchanged cadence for other kinds", () => {
    expect(paneInterval({ hidden: false, stateKind: "CommandRunning" })).toBe(
      PANE_INTERVALS.unchanged,
    );
  });
});
