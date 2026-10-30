import { describe, expect, it } from "vitest";
import { evalEtaSeconds, finishClock, finishLabel, formatElapsed, lossSeries, parseStartedMs, rowsEtaSeconds, stepsPerMinute } from "./trainingPageModel";

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


  it("parses the compact gpu-ctl started timestamp (#1523)", () => {
    const compactNow = Date.parse("2026-10-29T14:35:35Z");
    expect(stepsPerMinute(100, "20261029T143355", compactNow)).toBeCloseTo(60);
  });
});


describe("parseStartedMs", () => {
  it("parses ISO strings via Date.parse", () => {
    expect(parseStartedMs("2026-01-15T12:00:00Z")).toBe(Date.parse("2026-01-15T12:00:00Z"));
  });

  it("reads the compact gpu-ctl form as UTC", () => {
    expect(parseStartedMs("20261029T143355")).toBe(Date.parse("2026-10-29T14:33:55Z"));
  });

  it("returns null for empty, null or garbage input", () => {
    expect(parseStartedMs(null)).toBeNull();
    expect(parseStartedMs("")).toBeNull();
    expect(parseStartedMs("garbage")).toBeNull();
    expect(parseStartedMs("2026-13-99T99:99:99Z")).toBeNull();
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

describe("formatElapsed compact started timestamps (#1523)", () => {
  it("parses the compact gpu-ctl started timestamp instead of Date.parse", () => {
    const now = Date.parse("2026-10-29T14:44:55Z");
    expect(formatElapsed("20261029T143355", now)).toBe("11 min");
  });
});

describe("finishLabel", () => {
  const now = Date.parse("2026-01-15T12:00:00Z");

  it("returns just the clock when finishing on the same calendar day", () => {
    expect(finishLabel(60, now)).toBe(finishClock(60, now));
  });

  it("prefixes tomorrow for the next calendar day", () => {
    const target = new Date(now);
    target.setDate(target.getDate() + 1);
    target.setHours(8, 0, 0, 0);
    const eta = (target.getTime() - now) / 1000;
    expect(finishLabel(eta, now)).toBe(`tomorrow ${finishClock(eta, now)}`);
  });

  it("uses the short weekday for days beyond tomorrow", () => {
    const target = new Date(now);
    target.setDate(target.getDate() + 3);
    target.setHours(9, 0, 0, 0);
    const eta = (target.getTime() - now) / 1000;
    expect(finishLabel(eta, now)).toBe(`${["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][target.getDay()]} ${finishClock(eta, now)}`);
  });

  it("returns null when eta is null or not finite", () => {
    expect(finishLabel(null, now)).toBeNull();
    expect(finishLabel(Number.NaN, now)).toBeNull();
  });
});

describe("evalEtaSeconds", () => {
  const entry = (done: number, labels = 4, elapsedS: number | null = 120) => ({
    checkpoint_label: "ckpt",
    group: "grp",
    current: null,
    labels: Array.from({ length: labels }, (_, i) => `label-${i}`),
    done: Array.from({ length: done }, (_, i) => `label-${i}`),
    elapsed_s: elapsedS,
  });

  it("returns the max remaining seconds across qualifying entries", () => {
    expect(evalEtaSeconds([entry(2, 4, 120), entry(3, 6, 30)])).toBe(120);
  });

  it("skips entries without progress and returns null when none qualify", () => {
    expect(evalEtaSeconds([])).toBeNull();
    expect(evalEtaSeconds([entry(0), entry(2, 4, 0), entry(2, 4, null)])).toBeNull();
  });
});

describe("rowsEtaSeconds", () => {
  const t0 = Date.parse("2026-01-15T12:00:00Z");

  it("divides remaining rows by the observed rate", () => {
    expect(rowsEtaSeconds({ rows: 100, atMs: t0 }, { rows: 400, atMs: t0 + 60_000 }, 1000)).toBe(120);
  });

  it("returns null when target is null", () => {
    expect(rowsEtaSeconds({ rows: 100, atMs: t0 }, { rows: 400, atMs: t0 + 60_000 }, null)).toBeNull();
  });

  it("returns null when rows did not increase", () => {
    expect(rowsEtaSeconds({ rows: 400, atMs: t0 }, { rows: 400, atMs: t0 + 60_000 }, 1000)).toBeNull();
    expect(rowsEtaSeconds({ rows: 500, atMs: t0 }, { rows: 400, atMs: t0 + 60_000 }, 1000)).toBeNull();
  });

  it("returns null when the samples are under 60 seconds apart", () => {
    expect(rowsEtaSeconds({ rows: 100, atMs: t0 }, { rows: 400, atMs: t0 + 59_000 }, 1000)).toBeNull();
  });
});
