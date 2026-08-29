import { describe, it, expect } from "vitest";
import { FIXTURE_COMMANDS, FIXTURE_EVENTS, FIXTURE_SNAPSHOT, snapshotJson } from "./fixtures";
import { parseServerFrame, SESSION_PROTOCOL_VERSION } from "./public";

describe("webSessionFixtures", () => {
  it("mirrors every command variant", () => {
    expect(Object.keys(FIXTURE_COMMANDS)).toEqual([
      "prompt",
      "cancel",
      "cancelKeepQueue",
      "setModel",
      "permission",
    ]);
  });

  it("parses protocol v2 snapshot and event fixtures", () => {
    const snapshot = parseServerFrame(snapshotJson({ model: "composer-2.5", turnState: "busy" }));
    expect(snapshot).toEqual({
      kind: "snapshot",
      snapshot: {
        type: "snapshot",
        protocolVersion: SESSION_PROTOCOL_VERSION,
        cursor: 0,
        model: "composer-2.5",
        turnState: "busy",
        reset: false,
        contextState: "live",
        contextEpoch: 0,
      },
    });
    expect(parseServerFrame(JSON.stringify({ type: "event", protocolVersion: 2, cursor: 1, payload: FIXTURE_EVENTS.agentMessage }))).toEqual({
      kind: "event",
      cursor: 1,
      event: FIXTURE_EVENTS.agentMessage,
    });
    expect(FIXTURE_SNAPSHOT.busy.pendingPermission?.requestId).toBe("p1");
  });
});
