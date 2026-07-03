import { describe, it, expect } from "vitest";
import {
  IncompatibleResponseError,
  assertCockpit,
  assertOperationResponse,
  isTaskStatus,
} from "./contracts";

describe("isTaskStatus", () => {
  it("accepts the four canonical statuses", () => {
    for (const s of ["running", "waiting", "idle", "error"]) {
      expect(isTaskStatus(s)).toBe(true);
    }
  });
  it("rejects anything else", () => {
    expect(isTaskStatus("done")).toBe(false);
    expect(isTaskStatus(undefined)).toBe(false);
  });
});

describe("assertCockpit", () => {
  const valid = {
    backend: { authority: "host-native", control_enabled: true },
    repos: { repos: [] },
    cards: [],
    inbox: { items: [] },
  };

  it("accepts a well-formed cockpit", () => {
    expect(assertCockpit(valid).cards).toEqual([]);
  });

  it("rejects a non-object top level", () => {
    expect(() => assertCockpit(null)).toThrow(IncompatibleResponseError);
    expect(() => assertCockpit([])).toThrow(IncompatibleResponseError);
  });

  it("rejects a missing cards array", () => {
    expect(() => assertCockpit({ ...valid, cards: undefined })).toThrow(
      IncompatibleResponseError,
    );
  });

  it("rejects a card with an invalid status", () => {
    const bad = { ...valid, cards: [{ qualified_handle: "x/y", repo: "x", status: "nope", actions: [] }] };
    expect(() => assertCockpit(bad)).toThrow(IncompatibleResponseError);
  });

  it("rejects a malformed action", () => {
    const bad = {
      ...valid,
      cards: [
        {
          qualified_handle: "x/y",
          repo: "x",
          status: "idle",
          actions: [{ label: "no action id" }],
        },
      ],
    };
    expect(() => assertCockpit(bad)).toThrow(IncompatibleResponseError);
  });
});

describe("assertOperationResponse", () => {
  it("accepts a production operation envelope", () => {
    const response = assertOperationResponse({
      ok: true,
      state_changed: true,
      output: "done",
      cockpit: {
        backend: { authority: "host-native", control_enabled: true },
        repos: { repos: [] },
        cards: [],
        inbox: { items: [] },
      },
    });

    expect(response.ok).toBe(true);
  });

  it("accepts a server confirmation token on conflict responses", () => {
    const response = assertOperationResponse({
      ok: false,
      request_id: "drop-1",
      state_changed: false,
      error: "confirmation required",
      confirmation_token: "confirm-token",
    });

    expect(response.confirmation_token).toBe("confirm-token");
  });

  it("rejects a malformed nested cockpit projection", () => {
    expect(() =>
      assertOperationResponse({
        ok: true,
        state_changed: true,
        cockpit: { cards: "not-an-array" },
      }),
    ).toThrow(IncompatibleResponseError);
  });

  it("rejects malformed envelope fields", () => {
    expect(() => assertOperationResponse({ ok: "yes" })).toThrow(
      IncompatibleResponseError,
    );
    expect(() => assertOperationResponse({ ok: false, error: 42 })).toThrow(
      IncompatibleResponseError,
    );
    expect(() =>
      assertOperationResponse({ ok: false, confirmation_token: 42 }),
    ).toThrow(IncompatibleResponseError);
  });
});
