/**
 * Refit scheduling for the raw terminal host.
 *
 * Layout events arrive in bursts (keyboard animation frames, rotation,
 * pinch font steps), and the two effects they drive have opposite costs:
 * a local grid fit is cheap, while a server resize SIGWINCHes the shared
 * tmux window and corrupts the pane if sprayed. The scheduler owns that
 * asymmetry — fits coalesce per animation frame; server resizes either
 * flush with the fit (connection time, discrete layout jumps) or collapse
 * behind a debounce until the burst settles. The grid fit and the PTY
 * resize are injected, so the policy is unit-testable without a DOM or a
 * terminal.
 */

/** Quiet window a layout-event burst must clear before the PTY is resized. */
export const RESIZE_DEBOUNCE_MS = 300;

export interface RefitScheduler {
  /** Fit and notify the PTY on the next frame. Connection-time path: the
   * PTY must learn the real size immediately, so no debounce. */
  scheduleImmediate(): void;
  /** Fit on the next frame; debounce the server resize so an event burst
   * (e.g. the keyboard animation) collapses into a single resize. */
  scheduleDebounced(): void;
  /** Debounced refit now and again a frame later: renderer cell metrics
   * settle one frame after a font-size change. */
  scheduleFontSize(): void;
  /** Immediate refit now and again a frame later: discrete layout jumps
   * (the expand toggle) re-measure once the new layout has painted. */
  schedulePostLayout(): void;
  /** Cancel pending work; every later schedule call becomes a no-op. */
  dispose(): void;
}

export function createRefitScheduler(host: {
  /** Fit the local grid to the host (never resizes the PTY). */
  fit(): void;
  /** Push the fitted size to the PTY/server. */
  sendResize(): void;
}): RefitScheduler {
  let disposed = false;
  let fitFrame = 0;
  let fontSizeFrame = 0;
  let resizeTimer: ReturnType<typeof setTimeout> | undefined;

  // One shared frame for both paths: an immediate request supersedes a
  // pending debounced one (and vice versa) instead of double-fitting.
  const scheduleFit = (resizeWithFit: boolean) => {
    if (fitFrame) cancelAnimationFrame(fitFrame);
    fitFrame = requestAnimationFrame(() => {
      fitFrame = 0;
      if (disposed) return;
      host.fit();
      if (resizeWithFit) host.sendResize();
    });
  };

  const scheduleImmediate = () => {
    if (disposed) return;
    scheduleFit(true);
  };

  const scheduleDebounced = () => {
    if (disposed) return;
    scheduleFit(false);
    if (resizeTimer) clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => {
      resizeTimer = undefined;
      host.sendResize();
    }, RESIZE_DEBOUNCE_MS);
  };

  const scheduleFontSize = () => {
    if (disposed) return;
    if (fontSizeFrame) cancelAnimationFrame(fontSizeFrame);
    scheduleDebounced();
    fontSizeFrame = requestAnimationFrame(() => {
      fontSizeFrame = 0;
      if (!disposed) scheduleDebounced();
    });
  };

  const schedulePostLayout = () => {
    if (disposed) return;
    scheduleImmediate();
    requestAnimationFrame(() => {
      if (!disposed) scheduleImmediate();
    });
  };

  const dispose = () => {
    disposed = true;
    if (fitFrame) cancelAnimationFrame(fitFrame);
    if (fontSizeFrame) cancelAnimationFrame(fontSizeFrame);
    if (resizeTimer) clearTimeout(resizeTimer);
    fitFrame = 0;
    fontSizeFrame = 0;
    resizeTimer = undefined;
  };

  return { scheduleImmediate, scheduleDebounced, scheduleFontSize, schedulePostLayout, dispose };
}
