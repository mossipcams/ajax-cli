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

type ScalarCase = {
  family: string;
  value: number;
};

type FuzzCase = ScalarCase & {
  seed: number;
  iteration: number;
};

const ADVERSARIAL: ScalarCase[] = [
  { family: "nan", value: NaN },
  { family: "infinity", value: Infinity },
  { family: "negative-infinity", value: -Infinity },
  { family: "zero", value: 0 },
  { family: "negative-zero", value: -0 },
  { family: "negative", value: -1 },
  { family: "huge-negative", value: -9999 },
  { family: "huge-finite", value: 1e9 },
  { family: "sub-pixel", value: 0.5 },
  { family: "below-min-cols", value: 79 },
  { family: "exact-min-cols", value: 80 },
  { family: "above-min-cols", value: 81 },
  { family: "phone-width", value: 390 },
];

const REQUIRED_ADVERSARIAL_LABELS = [
  "nan",
  "infinity",
  "negative-infinity",
  "zero",
  "negative-zero",
  "exact-min-cols",
  "sub-pixel",
  "huge-finite",
  "phone-width",
];

function adversarialLabelsForTest(): string[] {
  return ADVERSARIAL.map(({ family }) => family);
}

function randomScalarCase(rng: () => number): ScalarCase {
  if (rng() < 0.15) {
    return ADVERSARIAL[Math.floor(rng() * ADVERSARIAL.length)]!;
  }
  const value = rng() * 2000 - 1000;
  return {
    family: value < 0 ? "generated-negative" : "generated-positive",
    value,
  };
}

function sampleFuzzCasesForTest(seed: number, count: number): FuzzCase[] {
  const rng = makeRng(seed);
  return Array.from({ length: count }, (_, iteration) => {
    const scalar = randomScalarCase(rng);
    return { seed, iteration, ...scalar };
  });
}

function pick(rng: () => number): number {
  return randomScalarCase(rng).value;
}

function isFiniteNumber(n: unknown): n is number {
  return typeof n === "number" && Number.isFinite(n);
}

function expectedFlooredCols(
  proposedCols: number | undefined,
  minCols: number,
): number {
  if (
    proposedCols === undefined ||
    !Number.isFinite(proposedCols) ||
    proposedCols <= 0
  ) {
    return minCols;
  }
  return Math.max(proposedCols, minCols);
}

function expectedClampPan(
  panPx: number,
  contentPx: number,
  viewportPx: number,
): number {
  if (
    !Number.isFinite(panPx) ||
    !Number.isFinite(contentPx) ||
    !Number.isFinite(viewportPx)
  ) {
    return 0;
  }
  const maxPan = Math.max(0, contentPx - viewportPx);
  return Math.min(Math.max(panPx, 0), maxPan);
}

function expectedFitCapFontSize(
  currentFontSize: number,
  proposedCols: number | undefined,
  minCols: number,
  min: number = MIN_FONT_SIZE,
  max: number = MAX_FONT_SIZE,
): number {
  if (
    proposedCols === undefined ||
    !Number.isFinite(proposedCols) ||
    proposedCols <= 0 ||
    !Number.isFinite(currentFontSize) ||
    currentFontSize <= 0
  ) {
    return max;
  }
  const cap = Math.floor((currentFontSize * proposedCols) / minCols);
  return Math.min(Math.max(cap, min), max);
}

function expectedPinchActivated(
  startDistancePx: number,
  currentDistancePx: number,
  thresholdPx: number,
): boolean {
  if (
    !Number.isFinite(startDistancePx) ||
    !Number.isFinite(currentDistancePx) ||
    startDistancePx <= 0 ||
    currentDistancePx <= 0
  ) {
    return false;
  }
  return Math.abs(currentDistancePx - startDistancePx) >= thresholdPx;
}

function expectedPinchFontSize(
  baseFontSize: number,
  startDistancePx: number,
  currentDistancePx: number,
  min: number = MIN_FONT_SIZE,
  max: number = MAX_FONT_SIZE,
): number {
  if (
    !Number.isFinite(startDistancePx) ||
    !Number.isFinite(currentDistancePx) ||
    startDistancePx <= 0 ||
    currentDistancePx <= 0
  ) {
    return baseFontSize;
  }
  const scaled = Math.round(
    baseFontSize * (currentDistancePx / startDistancePx),
  );
  return Math.min(Math.max(scaled, min), max);
}

type ComposedGeometryScenario = {
  currentFont: number;
  proposedCols: number;
  minCols: number;
  contentPx: number;
  viewportPx: number;
  panPx: number;
  nextContentPx: number;
  nextViewportPx: number;
  baseFont: number;
  pinchStart: number;
  pinchCurrent: number;
  pinchMax: number;
};

function finiteLayoutPx(rng: () => number): number {
  return 280 + rng() * 500;
}

function maybeAdversarialLayoutPx(rng: () => number): number {
  return rng() < 0.25 ? pick(rng) : finiteLayoutPx(rng);
}

function sampleComposedGeometryScenarios(
  seed: number,
  count: number,
): ComposedGeometryScenario[] {
  const rng = makeRng(seed);
  return Array.from({ length: count }, (_, iteration) => {
    const minCols = iteration % 2 === 0 ? MIN_TERMINAL_COLS : 40;
    const proposedCols = pick(rng);
    const currentFont = pick(rng);
    const cap = expectedFitCapFontSize(currentFont, proposedCols, minCols);
    const pinchMax = Math.min(
      MAX_FONT_SIZE,
      Math.max(
        MIN_FONT_SIZE,
        iteration % 3 === 0
          ? cap
          : MIN_FONT_SIZE + rng() * (MAX_FONT_SIZE - MIN_FONT_SIZE),
      ),
    );

    return {
      currentFont,
      proposedCols,
      minCols,
      contentPx: maybeAdversarialLayoutPx(rng),
      viewportPx: maybeAdversarialLayoutPx(rng),
      panPx: pick(rng),
      nextContentPx: finiteLayoutPx(rng),
      nextViewportPx: finiteLayoutPx(rng),
      baseFont: pick(rng),
      pinchStart: pick(rng),
      pinchCurrent: pick(rng),
      pinchMax,
    };
  });
}

describe("terminalGeometry fuzz invariants", () => {
  it("uses labeled replayable fuzz cases and adversarial corpus coverage", () => {
    const cases = sampleFuzzCasesForTest(0x5eed, 8);

    expect(cases).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          seed: 0x5eed,
          iteration: expect.any(Number),
          family: expect.not.stringMatching(/^unlabeled$/),
          value: expect.any(Number),
        }),
      ]),
    );
    expect(adversarialLabelsForTest()).toEqual(
      expect.arrayContaining(REQUIRED_ADVERSARIAL_LABELS),
    );
  });

  it("matches independent geometry oracles across generated cases", () => {
    const cases = [
      ...sampleFuzzCasesForTest(0x0c1e, 12),
      ...ADVERSARIAL.map((scalar, iteration) => ({
        seed: 0xadbe,
        iteration,
        ...scalar,
      })),
    ];
    const rng = makeRng(0x0c1e);
    const minCols = MIN_TERMINAL_COLS;
    const customFitMin = 10;
    const customFitMax = 22;
    const customPinchMin = 8;
    const customPinchMax = 28;

    for (const fuzzCase of cases) {
      const { value } = fuzzCase;

      expect(flooredCols(value, minCols)).toBe(
        expectedFlooredCols(value, minCols),
      );

      const pan = pick(rng);
      const content = pick(rng);
      const viewport = pick(rng);
      expect(clampPan(pan, content, viewport)).toBe(
        expectedClampPan(pan, content, viewport),
      );

      const font = value;
      const fitCols = pick(rng);
      expect(fitCapFontSize(font, fitCols, minCols)).toBe(
        expectedFitCapFontSize(font, fitCols, minCols),
      );
      expect(
        fitCapFontSize(font, fitCols, minCols, customFitMin, customFitMax),
      ).toBe(
        expectedFitCapFontSize(
          font,
          fitCols,
          minCols,
          customFitMin,
          customFitMax,
        ),
      );

      const startDist = pick(rng);
      const currentDist = pick(rng);
      const threshold = pick(rng);
      expect(pinchActivated(startDist, currentDist, threshold)).toBe(
        expectedPinchActivated(startDist, currentDist, threshold),
      );

      const base = value;
      const pinchStart = pick(rng);
      const pinchCurrent = pick(rng);
      expect(pinchFontSize(base, pinchStart, pinchCurrent)).toBe(
        expectedPinchFontSize(base, pinchStart, pinchCurrent),
      );
      expect(
        pinchFontSize(
          base,
          pinchStart,
          pinchCurrent,
          customPinchMin,
          customPinchMax,
        ),
      ).toBe(
        expectedPinchFontSize(
          base,
          pinchStart,
          pinchCurrent,
          customPinchMin,
          customPinchMax,
        ),
      );
    }
  });

  it("preserves composed terminal geometry scenarios", () => {
    const scenarios = sampleComposedGeometryScenarios(0xface, 16);

    for (const scenario of scenarios) {
      const {
        currentFont,
        proposedCols,
        minCols,
        contentPx,
        viewportPx,
        panPx,
        nextContentPx,
        nextViewportPx,
        baseFont,
        pinchStart,
        pinchCurrent,
        pinchMax,
      } = scenario;

      const cap = fitCapFontSize(currentFont, proposedCols, minCols);
      expect(cap).toBeGreaterThanOrEqual(MIN_FONT_SIZE);
      expect(cap).toBeLessThanOrEqual(MAX_FONT_SIZE);
      expect(cap).toBe(
        expectedFitCapFontSize(currentFont, proposedCols, minCols),
      );

      const cols = flooredCols(proposedCols, minCols);
      expect(cols).toBeGreaterThanOrEqual(minCols);
      expect(cols).toBe(expectedFlooredCols(proposedCols, minCols));

      const initialPan = clampPan(panPx, contentPx, viewportPx);
      const resizedPan = clampPan(initialPan, nextContentPx, nextViewportPx);
      const maxResizedPan = Math.max(0, nextContentPx - nextViewportPx);
      expect(resizedPan).toBeGreaterThanOrEqual(0);
      expect(resizedPan).toBeLessThanOrEqual(maxResizedPan);
      expect(resizedPan).toBe(
        expectedClampPan(initialPan, nextContentPx, nextViewportPx),
      );

      const pinched = pinchFontSize(
        baseFont,
        pinchStart,
        pinchCurrent,
        MIN_FONT_SIZE,
        pinchMax,
      );
      expect(pinched).toBe(
        expectedPinchFontSize(
          baseFont,
          pinchStart,
          pinchCurrent,
          MIN_FONT_SIZE,
          pinchMax,
        ),
      );
      const pinchDistancesValid =
        Number.isFinite(pinchStart) &&
        Number.isFinite(pinchCurrent) &&
        pinchStart > 0 &&
        pinchCurrent > 0;
      if (!pinchDistancesValid) {
        expect(pinched).toBe(baseFont);
      }
    }
  });

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
