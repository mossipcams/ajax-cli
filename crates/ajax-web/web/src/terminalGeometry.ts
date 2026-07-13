/**
 * Pure sizing math for the mobile terminal.
 *
 * On a phone the viewport fits far fewer than the ~80 columns the hosted
 * tmux/Claude Code TUI assumes. Agent-sized fit geometry keeps the PTY at
 * least 80 columns and CSS-scales the terminal element to the host width so
 * live and scrollback share the same layout without mid-token soft wrap.
 * These helpers own the column floor, scale-to-fit, the fit-to-width font cap,
 * the horizontal pan clamp, and the pinch-distance → font-size mapping so the
 * gesture wiring in TerminalRawView stays thin and the math stays unit-testable.
 */

/** Fallback column floor for callers that pass an invalid minimum. */
export const MIN_TERMINAL_COLS = 80;

/** Legacy fit floor; prefer {@link logicalCols} with {@link MIN_TERMINAL_COLS}. */
export const FIT_TERMINAL_COLS = 40;

/** Pinch-zoom font bounds: below 7px text is unreadable, above 20px the
 * canvas outgrows a phone gesture's usefulness. */
export const MIN_FONT_SIZE = 7;
export const MAX_FONT_SIZE = 20;

/** The default cell size on every viewport. */
export const DEFAULT_FONT_SIZE = 13;

/** Ghostty scrollback line caps: lighter on phones, full history on desktop. */
export const MOBILE_SCROLLBACK_LINES = 2000;
export const DESKTOP_SCROLLBACK_LINES = 10000;

/** Matches TaskDetail / TerminalRawView mobile CSS media query. */
const MOBILE_MEDIA_QUERY =
  "(max-width: 767px), (pointer: coarse) and (max-height: 500px)";

/**
 * Scrollback lines for the Ghostty constructor. Uses the same mobile media
 * heuristic as TaskDetail CSS so phone viewports get the lighter buffer.
 */
export function terminalScrollbackLines(): number {
  if (typeof window !== "undefined" && window.matchMedia?.(MOBILE_MEDIA_QUERY).matches) {
    return MOBILE_SCROLLBACK_LINES;
  }
  return DESKTOP_SCROLLBACK_LINES;
}

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

function sanitizeFontBounds(min: number, max: number): [number, number] {
  let lo = Number.isFinite(min) ? min : MIN_FONT_SIZE;
  let hi = Number.isFinite(max) ? max : MAX_FONT_SIZE;
  if (lo > hi) {
    lo = MIN_FONT_SIZE;
    hi = MAX_FONT_SIZE;
  }
  return [lo, hi];
}

/**
 * Raise a fitted column proposal to the column floor. Invalid proposals
 * (absent, non-finite, or non-positive — e.g. pre-layout fits) get the floor.
 * Valid proposals are floored to whole columns before applying the floor.
 */
export function flooredCols(proposedCols: number | undefined, minCols: number): number {
  const floor = Number.isFinite(minCols) ? Math.floor(minCols) : MIN_TERMINAL_COLS;
  if (proposedCols === undefined || !Number.isFinite(proposedCols) || proposedCols <= 0) {
    return floor;
  }
  return Math.max(Math.floor(proposedCols), floor);
}

/**
 * Agent-sized column count: raise a host-fit proposal to {@link MIN_TERMINAL_COLS}.
 */
export function logicalCols(hostFitCols: number | undefined): number {
  return flooredCols(hostFitCols, MIN_TERMINAL_COLS);
}

/**
 * CSS scale that fits a logical `cols * cellWidth` canvas into `hostWidthPx`.
 * Returns 1 when the canvas already fits or measurements are invalid.
 */
export function fitScale(
  hostWidthPx: number,
  cols: number,
  cellWidthPx: number,
): number {
  if (
    !Number.isFinite(hostWidthPx) ||
    hostWidthPx <= 0 ||
    !Number.isFinite(cols) ||
    cols <= 0 ||
    !Number.isFinite(cellWidthPx) ||
    cellWidthPx <= 0
  ) {
    return 1;
  }
  const logicalWidth = cols * cellWidthPx;
  if (logicalWidth <= 0) return 1;
  return Math.min(1, hostWidthPx / logicalWidth);
}

/** Whole-row count from a fit proposal; invalid proposals fall back to 24. */
export function logicalRows(proposedRows: number | undefined): number {
  if (proposedRows === undefined || !Number.isFinite(proposedRows) || proposedRows <= 0) {
    return 24;
  }
  return Math.floor(proposedRows);
}

/**
 * Raise logical rows when CSS scale shrinks the canvas below the host height.
 * Invalid or unity scale leaves rows unchanged.
 */
export function scaledLogicalRows(proposedRows: number | undefined, scale: number): number {
  if (proposedRows === undefined || !Number.isFinite(proposedRows) || proposedRows <= 0) {
    return logicalRows(proposedRows);
  }
  const rows = logicalRows(proposedRows);
  if (!Number.isFinite(scale) || scale <= 0 || scale >= 1) {
    return rows;
  }
  return Math.max(1, Math.ceil(rows / scale));
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
  const [lo, hi] = sanitizeFontBounds(min, max);
  if (
    proposedCols === undefined ||
    !Number.isFinite(proposedCols) ||
    proposedCols <= 0 ||
    !Number.isFinite(currentFontSize) ||
    currentFontSize <= 0
  ) {
    return hi;
  }
  const cap = Math.floor((currentFontSize * proposedCols) / minCols);
  return Math.min(Math.max(cap, lo), hi);
}

/**
 * True once a two-finger gesture has moved far enough to count as a deliberate
 * pinch-zoom. A small incidental change in finger distance (a graze, a resting
 * second finger) stays below the deadzone so it never rewraps the terminal.
 * Non-finite or non-positive distances are never activated.
 */
export function pinchActivated(
  startDistancePx: number,
  currentDistancePx: number,
  thresholdPx: number,
): boolean {
  if (
    !Number.isFinite(startDistancePx) ||
    !Number.isFinite(currentDistancePx) ||
    startDistancePx <= 0 ||
    currentDistancePx <= 0 ||
    !Number.isFinite(thresholdPx) ||
    thresholdPx < 0
  ) {
    return false;
  }
  return Math.abs(currentDistancePx - startDistancePx) >= thresholdPx;
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
  const [lo, hi] = sanitizeFontBounds(min, max);
  if (
    !Number.isFinite(startDistancePx) ||
    !Number.isFinite(currentDistancePx) ||
    startDistancePx <= 0 ||
    currentDistancePx <= 0
  ) {
    return baseFontSize;
  }
  if (!Number.isFinite(baseFontSize) || baseFontSize <= 0) {
    return lo;
  }
  const scaled = Math.round(baseFontSize * (currentDistancePx / startDistancePx));
  return Math.min(Math.max(scaled, lo), hi);
}
