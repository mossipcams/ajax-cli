import { describe, expect, it } from "vitest";
import {
  assertConnectionState,
  connectionStateAllowsSend,
  connectionStateIsActive,
  initialConnectionState,
  type ConnectionState,
} from "./connectionState";

describe("connectionState", () => {
  it("starts in connecting", () => {
    expect(initialConnectionState()).toBe("connecting");
  });

  it("covers every phase in the closed union", () => {
    const states: ConnectionState[] = [
      "connecting",
      "connected",
      "waiting",
      "failed",
      "disposed",
    ];
    for (const state of states) {
      expect(assertConnectionState(state)).toBe(state);
    }
  });

  it("allows send only while connected", () => {
    expect(connectionStateAllowsSend("connected")).toBe(true);
    expect(connectionStateAllowsSend("connecting")).toBe(false);
    expect(connectionStateAllowsSend("waiting")).toBe(false);
    expect(connectionStateAllowsSend("failed")).toBe(false);
    expect(connectionStateAllowsSend("disposed")).toBe(false);
  });

  it("treats connecting, connected, and waiting as active", () => {
    expect(connectionStateIsActive("connecting")).toBe(true);
    expect(connectionStateIsActive("connected")).toBe(true);
    expect(connectionStateIsActive("waiting")).toBe(true);
    expect(connectionStateIsActive("failed")).toBe(false);
    expect(connectionStateIsActive("disposed")).toBe(false);
  });
});
