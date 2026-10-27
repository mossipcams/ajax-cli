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
      },
    });
    expect(parseServerFrame(JSON.stringify({ type: "event", protocolVersion: 2, cursor: 1, payload: FIXTURE_EVENTS.agentMessage }))).toEqual({
      kind: "event",
      cursor: 1,
      event: FIXTURE_EVENTS.agentMessage,
    });
    expect(FIXTURE_SNAPSHOT.busy.pendingPermission?.requestId).toBe("p1");
  });

  it("parses the optional transcriptError from host snapshots", () => {
    const failed = parseServerFrame(
      JSON.stringify({
        type: "snapshot",
        protocolVersion: SESSION_PROTOCOL_VERSION,
        cursor: 0,
        model: "auto",
        turnState: "idle",
        reset: false,
        transcriptError: "forced append failure",
      }),
    );
    expect(failed).toMatchObject({
      kind: "snapshot",
      snapshot: { transcriptError: "forced append failure" },
    });
    const healthy = parseServerFrame(snapshotJson());
    expect(healthy?.kind === "snapshot" && "transcriptError" in healthy.snapshot).toBe(false);
  });

  it("parses promptsBlocked only when the host sets it true", () => {
    const base = {
      type: "snapshot",
      protocolVersion: SESSION_PROTOCOL_VERSION,
      cursor: 0,
      model: "auto",
      turnState: "idle",
      reset: false,
      transcriptError: "forced append failure",
    };
    const blocked = parseServerFrame(JSON.stringify({ ...base, promptsBlocked: true }));
    expect(blocked).toMatchObject({ kind: "snapshot", snapshot: { promptsBlocked: true } });
    const notice = parseServerFrame(JSON.stringify(base));
    expect(notice?.kind === "snapshot" && "promptsBlocked" in notice.snapshot).toBe(false);
  });
});
