import { describe, it, expect } from "vitest";
import {
  IncompatibleResponseError,
  assertCockpit,
  assertOperationResponse,
  isTaskStatus,
} from "./contracts";

describe("isTaskStatus", () => {
  it("accepts the five canonical statuses", () => {
    for (const s of ["running", "waiting", "idle", "error", "unknown"]) {
      expect(isTaskStatus(s)).toBe(true);
    }
  });
  it("rejects anything else", () => {
    expect(isTaskStatus("done")).toBe(false);
    expect(isTaskStatus(undefined)).toBe(false);
  });
});

describe("assertCockpit unknown status", () => {
  it("accepts a card with status unknown", () => {
    const cockpit = {
      backend: { authority: "host-native", control_enabled: true },
      repos: { repos: [] },
      cards: [
        {
          qualified_handle: "x/y",
          repo: "x",
          title: "y",
          status: "unknown",
          attention: "idle",
          last_activity_unix_secs: 0,
          actions: [],
        },
      ],
      inbox: { items: [] },
    };
    expect(assertCockpit(cockpit).cards[0].status).toBe("unknown");
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
          attention: "idle",
          actions: [{ label: "no action id" }],
        },
      ],
    };
    expect(() => assertCockpit(bad)).toThrow(IncompatibleResponseError);
  });

  it("assert_cockpit_rejects_unknown_attention_band", () => {
    const bad = {
      ...valid,
      cards: [
        {
          qualified_handle: "x/y",
          repo: "x",
          title: "y",
          status: "idle",
          attention: "sideways",
          last_activity_unix_secs: 0,
          actions: [],
        },
      ],
    };
    expect(() => assertCockpit(bad)).toThrow(IncompatibleResponseError);
  });

  it("validates optional branch adoption metadata", () => {
    const withAdoption = {
      ...valid,
      cards: [
        {
          qualified_handle: "x/y",
          repo: "x",
          status: "idle",
          attention: "idle",
          actions: [
            {
              action: "repair",
              label: "Repair",
              destructive: false,
              confirmation_required: true,
              branch_adoption: {
                expected_branch: "ajax/fix-login",
                observed_branch: "fix/pane-stuck",
              },
            },
          ],
        },
      ],
    };
    expect(assertCockpit(withAdoption).cards[0].actions[0].branch_adoption).toEqual({
      expected_branch: "ajax/fix-login",
      observed_branch: "fix/pane-stuck",
    });

    const missingObserved = {
      ...withAdoption,
      cards: [
        {
          ...withAdoption.cards[0],
          actions: [
            {
              ...withAdoption.cards[0].actions[0],
              branch_adoption: { expected_branch: "ajax/fix-login" },
            },
          ],
        },
      ],
    };
    expect(() => assertCockpit(missingObserved)).toThrow(IncompatibleResponseError);

    const nonStringObserved = {
      ...withAdoption,
      cards: [
        {
          ...withAdoption.cards[0],
          actions: [
            {
              ...withAdoption.cards[0].actions[0],
              branch_adoption: {
                expected_branch: "ajax/fix-login",
                observed_branch: 42,
              },
            },
          ],
        },
      ],
    };
    expect(() => assertCockpit(nonStringObserved)).toThrow(IncompatibleResponseError);
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
  });
});
