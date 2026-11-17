import { releaseVelocity } from "./navigateSwipe";

export const SHEET_DISMISS_THRESHOLD = 96;
export const SHEET_FLICK_VELOCITY = 0.5;
const RUBBER_BAND_LIMIT = 100;
const RUBBER_BAND_STIFFNESS = 0.55;

export interface SheetSample {
  t: number;
  y: number;
}

export function sheetOffset(dy: number): number {
  if (dy >= 0) return dy;
  const damped = 1 - 1 / ((-dy * RUBBER_BAND_STIFFNESS) / RUBBER_BAND_LIMIT + 1);
  return -damped * RUBBER_BAND_LIMIT;
}

export function sheetReleaseVelocity(samples: readonly SheetSample[], now: number): number {
  return releaseVelocity(
    samples.map((sample) => ({ t: sample.t, x: sample.y })),
    now,
  );
}

export function sheetDismisses(offset: number, velocity: number): boolean {
  return offset >= SHEET_DISMISS_THRESHOLD || (offset > 0 && velocity >= SHEET_FLICK_VELOCITY);
}
