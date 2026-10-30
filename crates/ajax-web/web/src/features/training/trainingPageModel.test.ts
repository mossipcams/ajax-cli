import { describe, expect, it } from "vitest";
import { finishClock, lossSeries, stepsPerMinute } from "./trainingPageModel";

describe("lossSeries", () => {
  it("returns loss values in line order from dict-style logs", () => {
    expect(lossSeries([
      "{'loss': 0.9, 'grad_norm': 0.8, 'epoch': 0.1}",
      "noise without a number",
      '{"loss": 0.5523, "learning_rate": 1.8e-4}',
    ])).toEqual([0.9, 0.5523]);
  });

  it("returns an empty array when no line matches", () => {
    expect(lossSeries([])).toEqual([]);
    expect(lossSeries(["Step 3 of 10", "{'grad_norm': 0.81}"])).toEqual([]);
  });

  it("ignores eval_loss and train_loss keys", () => {
    expect(lossSeries([
      "{'eval_loss': 0.31}",
      "{'train_loss': 0.42}",
      "{'loss': 0.6}",
    ])).toEqual([0.6]);
  });

  it("parses exponent forms", () => {
    expect(lossSeries(["{'loss': 1.2e-3}", "{'loss': 5E+1}", "{'loss': 2.5e0}"])).toEqual([0.0012, 50, 2.5]);
  });

  it("skips loss keys that are not followed by a number", () => {
    expect(lossSeries(["{'loss': nan}", "{'loss': None}", "loss: 0.4"])).toEqual([]);
  });
});

describe("stepsPerMinute", () => {
  const now = Date.parse("2026-01-15T12:00:00Z");

  it("divides steps by elapsed minutes", () => {
    const started = new Date(now - 10 * 60 * 1000).toISOString();
    expect(stepsPerMinute(1200, started, now)).toBeCloseTo(120);
  });

  it("returns null when started is missing", () => {
    expect(stepsPerMinute(100, null, now)).toBeNull();
  });

  it("returns null for an invalid start", () => {
    expect(stepsPerMinute(100, "garbage", now)).toBeNull();
  });

  it("returns null when less than 60 seconds have elapsed", () => {
    const started = new Date(now - 59_000).toISOString();
    expect(stepsPerMinute(100, started, now)).toBeNull();
  });

  it("returns null when step is 0", () => {
    const started = new Date(now - 10 * 60 * 1000).toISOString();
    expect(stepsPerMinute(0, started, now)).toBeNull();
  });
});

describe("finishClock", () => {
  const now = Date.parse("2026-01-15T12:00:00Z");

  it("returns a non-empty local time that differs for different etas", () => {
    const soon = finishClock(600, now);
    const later = finishClock(7200, now);
    expect(soon).toBeTruthy();
    expect(later).toBeTruthy();
    expect(soon).not.toBe(later);
  });

  it("returns null when eta is null", () => {
    expect(finishClock(null, now)).toBeNull();
  });

  it("returns null when eta is not finite", () => {
    expect(finishClock(Number.NaN, now)).toBeNull();
  });
});
