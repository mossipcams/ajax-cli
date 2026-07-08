import { describe, expect, it } from "vitest";
import {
  MIN_TERMINAL_COLS,
  MIN_FONT_SIZE,
  MAX_FONT_SIZE,
  flooredCols,
  clampPan,
  fitCapFontSize,
  pinchActivated,
  pinchFontSize,
} from "./terminalGeometry";

/** Seeded LCG for reproducible fuzz without extra dependencies. */
function makeRng(seed: number) {
  let state = seed >>> 0;
  return () => {
    state = (state * 1664525 + 1013904223) >>> 0;
    return state / 0x100000000;
  };
}

const ADVERSARIAL = [
  NaN,
  Infinity,
  -Infinity,
  0,
  -1,
  -9999,
  1e9,
  0.5,
  79,
  80,
  81,
];

function pick(rng: () => number): number {
  if (rng() < 0.15) {
    return ADVERSARIAL[Math.floor(rng() * ADVERSARIAL.length)]!;
  }
  return rng() * 2000 - 1000;
}

function isFiniteNumber(n: unknown): n is number {
  return typeof n === "number" && Number.isFinite(n);
}

describe("terminalGeometry fuzz invariants", () => {
  it("holds across ~500 randomized and adversarial inputs", () => {
    const rng = makeRng(0x5eed);
    const minCols = MIN_TERMINAL_COLS;

    for (let i = 0; i < 500; i++) {
      const cols = pick(rng);
      const floored = flooredCols(cols, minCols);
      expect(Number.isFinite(floored)).toBe(true);
      expect(floored).toBeGreaterThanOrEqual(minCols);
      if (!Number.isFinite(cols) || cols <= 0) {
        expect(floored).toBe(minCols);
      }

      const pan = pick(rng);
      const content = pick(rng);
      const viewport = pick(rng);
      const clamped = clampPan(pan, content, viewport);
      expect(Number.isFinite(clamped)).toBe(true);
      expect(clamped).toBeGreaterThanOrEqual(0);
      if (
        Number.isFinite(pan) &&
        Number.isFinite(content) &&
        Number.isFinite(viewport)
      ) {
        expect(clamped).toBeLessThanOrEqual(Math.max(0, content - viewport));
      } else {
        expect(clamped).toBe(0);
      }

      const font = pick(rng);
      const fitCols = pick(rng);
      const capped = fitCapFontSize(font, fitCols, minCols);
      expect(Number.isFinite(capped)).toBe(true);
      expect(capped).toBeGreaterThanOrEqual(MIN_FONT_SIZE);
      expect(capped).toBeLessThanOrEqual(MAX_FONT_SIZE);
      if (
        Number.isFinite(font) &&
        font > 0 &&
        Number.isFinite(fitCols) &&
        fitCols > 0
      ) {
        expect(Number.isInteger(capped)).toBe(true);
      } else {
        expect(capped).toBe(MAX_FONT_SIZE);
      }

      const startDist = pick(rng);
      const currentDist = pick(rng);
      const threshold = pick(rng);
      const activated = pinchActivated(startDist, currentDist, threshold);
      expect(typeof activated).toBe("boolean");
      if (
        !Number.isFinite(startDist) ||
        !Number.isFinite(currentDist) ||
        startDist <= 0 ||
        currentDist <= 0
      ) {
        expect(activated).toBe(false);
      }

      const base = pick(rng);
      const pinchStart = pick(rng);
      const pinchCurrent = pick(rng);
      const pinched = pinchFontSize(base, pinchStart, pinchCurrent);
      const pinchDistancesValid =
        Number.isFinite(pinchStart) &&
        Number.isFinite(pinchCurrent) &&
        pinchStart > 0 &&
        pinchCurrent > 0;
      if (!pinchDistancesValid) {
        expect(pinched).toBe(base);
      } else if (Number.isFinite(base) && base > 0) {
        expect(Number.isFinite(pinched)).toBe(true);
        expect(pinched).toBeGreaterThanOrEqual(MIN_FONT_SIZE);
        expect(pinched).toBeLessThanOrEqual(MAX_FONT_SIZE);
      }

      const outputs = [floored, clamped, capped] as number[];
      if (pinchDistancesValid && Number.isFinite(base) && base > 0) {
        outputs.push(pinched);
      }
      for (const value of outputs) {
        expect(isFiniteNumber(value)).toBe(true);
      }
    }
  });
});
