import { describe, it, expect } from "vitest";
import {
  STATUS_ORDER,
  statusRank,
  filterByProject,
  sortCards,
  isConfirmExpired,
  statusMeta,
  severityBucket,
} from "./state";
import type { BrowserTaskCard } from "./types";

function card(handle: string, status: BrowserTaskCard["status"]): BrowserTaskCard {
  return {
    id: handle,
    qualified_handle: handle,
    repo: handle.split("/")[0],
    title: handle,
    status,
    actions: [],
  };
}

describe("status ordering (presentation only)", () => {
  it("ranks running, waiting, error, idle", () => {
    expect(STATUS_ORDER).toEqual(["running", "waiting", "error", "idle"]);
    expect(statusRank("running")).toBeLessThan(statusRank("waiting"));
    expect(statusRank("waiting")).toBeLessThan(statusRank("error"));
    expect(statusRank("error")).toBeLessThan(statusRank("idle"));
  });

  it("sorts by status rank then handle", () => {
    const cards = [
      card("web/b", "idle"),
      card("web/a", "running"),
      card("web/c", "running"),
    ];
    expect(sortCards(cards).map((c) => c.qualified_handle)).toEqual([
      "web/a",
      "web/c",
      "web/b",
    ]);
  });
});

describe("filterByProject", () => {
  const cards = [card("web/a", "idle"), card("api/b", "idle")];

  it("returns all cards when no project is selected", () => {
    expect(filterByProject(cards, null)).toHaveLength(2);
  });

  it("filters by explicit repo identity", () => {
    expect(filterByProject(cards, "web").map((c) => c.qualified_handle)).toEqual([
      "web/a",
    ]);
  });
});

describe("isConfirmExpired", () => {
  it("expires once now passes the deadline", () => {
    expect(isConfirmExpired({ expiresAt: 1000 }, 999)).toBe(false);
    expect(isConfirmExpired({ expiresAt: 1000 }, 1001)).toBe(true);
  });
});

describe("statusMeta", () => {
  it("maps canonical statuses to tone and label", () => {
    expect(statusMeta("running")).toEqual({ tone: "running", label: "Running" });
    expect(statusMeta("error")).toEqual({ tone: "error", label: "Error" });
  });

  it("defaults to idle for unknown values", () => {
    expect(statusMeta("unknown")).toEqual({ tone: "idle", label: "Idle" });
  });
});

describe("severityBucket", () => {
  it("maps low severity numbers to high bucket", () => {
    expect(severityBucket(1)).toBe("high");
    expect(severityBucket(2)).toBe("high");
  });
  it("maps mid-range to medium", () => {
    expect(severityBucket(3)).toBe("medium");
  });
  it("maps high numbers to low urgency", () => {
    expect(severityBucket(5)).toBe("low");
  });
});
