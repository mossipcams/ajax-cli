import { describe, expect, it } from "vitest";
import { projectWireEvent, projectWireInput } from "./projectWireInput";

describe("projectWireInput", () => {
  it("maps agent messages to typed events without raw role strings", () => {
    expect(
      projectWireEvent({
        type: "message",
        role: "agent",
        text: "Hello",
        itemId: "i1",
      }),
    ).toEqual({ type: "agent_message", text: "Hello", itemId: "i1" });
  });

  it("drops artifacts at the projection boundary", () => {
    expect(projectWireEvent({ type: "artifact", kind: "x", title: "Modes", body: "{}" })).toBeNull();
  });

  it("maps ready frames to session_ready", () => {
    expect(projectWireEvent({ type: "ready", busy: false, reset: true })).toEqual({
      type: "session_ready",
      busy: false,
      reset: true,
    });
  });

  it("maps snapshot reset to session_ready", () => {
    expect(
      projectWireInput({
        type: "snapshot",
        protocolVersion: 2,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: true,
        contextState: "live",
        contextEpoch: 0,
      }),
    ).toEqual([{ type: "session_ready", reset: true, busy: false }]);
  });
});
