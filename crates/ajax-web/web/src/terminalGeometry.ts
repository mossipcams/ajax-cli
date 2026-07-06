/**
 * Pure sizing math for the mobile terminal's wide-canvas mode.
 *
 * On a phone the viewport fits far fewer than the ~80 columns the hosted
 * tmux/Claude Code TUI assumes. The column floor is mode-dependent: fit mode
 * (default) sizes the PTY to the visible width with a 40-column safety floor
 * so the grid stays readable without horizontal panning; wide mode keeps the
 * classic 80-column canvas where the font shrinks until the floor fits the
 * screen and horizontal pan covers sub-minimum overflow. These helpers own the
 * column floor, the fit-to-width font cap, the horizontal pan clamp, and the
 * pinch-distance → font-size mapping so the gesture wiring in TerminalRawView
 * stays thin and the math stays unit-testable.
 */

/** Never let the PTY drop below this many columns in wide geometry mode. */
export const MIN_TERMINAL_COLS = 80;

/** Safety floor for fit geometry mode (real phones fit 45+ cols in practice). */
export const FIT_TERMINAL_COLS = 40;

export type GeometryMode = "fit" | "wide";

const GEOMETRY_MODE_STORAGE_KEY = "ajax.terminal.geometryMode";

/** Pinch-zoom font bounds: below 7px text is unreadable, above 20px the
 * 80-col canvas outgrows any phone gesture's usefulness. */
export const MIN_FONT_SIZE = 7;
export const MAX_FONT_SIZE = 20;

/** The default cell size on every viewport — the 80-column floor made
 * per-device font sizing obsolete (narrow viewports pan instead of wrap). */
export const DEFAULT_FONT_SIZE = 13;

const FONT_SIZE_STORAGE_KEY = "ajax.terminal.fontSize";

/**
 * The operator's persisted pinch-zoom font choice; a valid stored size wins
 * over the default. localStorage can throw (Safari private mode), so reads
 * and writes are best-effort.
 */
export function persistedFontSize(): number | undefined {
  try {
    const raw = window.localStorage.getItem(FONT_SIZE_STORAGE_KEY);
    if (!raw) return undefined;
    const parsed = Number.parseInt(raw, 10);
    if (!Number.isFinite(parsed) || parsed < MIN_FONT_SIZE || parsed > MAX_FONT_SIZE) {
      return undefined;
    }
    return parsed;
  } catch {
    return undefined;
  }
}

export function persistFontSize(size: number): void {
  try {
    window.localStorage.setItem(FONT_SIZE_STORAGE_KEY, String(size));
  } catch {
    // Best-effort: the session still uses the new size.
  }
}

/**
 * The operator's persisted geometry mode choice; a valid stored mode wins over
 * the default. localStorage can throw (Safari private mode), so reads and
 * writes are best-effort.
 */
export function persistedGeometryMode(): GeometryMode | undefined {
  try {
    const raw = window.localStorage.getItem(GEOMETRY_MODE_STORAGE_KEY);
    if (raw === "fit" || raw === "wide") {
      return raw;
    }
    return undefined;
  } catch {
    return undefined;
  }
}

export function persistGeometryMode(mode: GeometryMode): void {
  try {
    window.localStorage.setItem(GEOMETRY_MODE_STORAGE_KEY, mode);
  } catch {
    // Best-effort: the session still uses the new mode.
  }
}

/**
 * Raise a fitted column proposal to the column floor. Invalid proposals
 * (absent, non-finite, or non-positive — e.g. pre-layout fits) get the floor.
 */
export function flooredCols(proposedCols: number | undefined, minCols: number): number {
  if (proposedCols === undefined || !Number.isFinite(proposedCols) || proposedCols <= 0) {
    return minCols;
  }
  return Math.max(proposedCols, minCols);
}

/**
 * Clamp a horizontal pan offset to the scrollable range
 * `[0, max(0, contentPx - viewportPx)]`. Non-finite inputs return 0 so a
 * bad measurement can never fling the canvas off-screen.
 */
export function clampPan(panPx: number, contentPx: number, viewportPx: number): number {
  if (!Number.isFinite(panPx) || !Number.isFinite(contentPx) || !Number.isFinite(viewportPx)) {
    return 0;
  }
  const maxPan = Math.max(0, contentPx - viewportPx);
  return Math.min(Math.max(panPx, 0), maxPan);
}

/**
 * The largest font size at which `minCols` columns still fit the width that
 * currently holds `proposedCols` columns at `currentFontSize`. Cell width
 * scales linearly with font size, so the host width is
 * `proposedCols * cellWidth(currentFontSize)` and the cap follows as
 * `floor(currentFontSize * proposedCols / minCols)`, clamped to the readable
 * font range. Invalid measurements (absent/non-positive proposals or font
 * sizes — e.g. pre-layout fits) return `max`: no measurement, no constraint.
 */
export function fitCapFontSize(
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

/**
 * Map a two-finger pinch to a font size: scale the size the gesture started
 * at by the finger-distance ratio, rounded and clamped. Zero/non-finite
 * distances (finger lift mid-gesture) leave the base size untouched.
 */
export function pinchFontSize(
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
  const scaled = Math.round(baseFontSize * (currentDistancePx / startDistancePx));
  return Math.min(Math.max(scaled, min), max);
}
