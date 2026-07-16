const RESIZE_DEBOUNCE_MS = 100;

export type RefitControllerDeps = {
  fit: () => void;
  readSize: () => { cols: number; rows: number } | null;
  sendResize: (cols: number, rows: number) => void;
};

export type RefitController = {
  requestRefit: () => void;
  noteReconnect: () => void;
  dispose: () => void;
};

export function createRefitController(deps: RefitControllerDeps): RefitController {
  let disposed = false;
  let fitFrameId = 0;
  let settlingFrameId = 0;
  let resizeTimer: ReturnType<typeof setTimeout> | undefined;
  let lastSentCols = 0;
  let lastSentRows = 0;

  const cancelPending = () => {
    if (fitFrameId) {
      cancelAnimationFrame(fitFrameId);
      fitFrameId = 0;
    }
    if (settlingFrameId) {
      cancelAnimationFrame(settlingFrameId);
      settlingFrameId = 0;
    }
    if (resizeTimer) {
      clearTimeout(resizeTimer);
      resizeTimer = undefined;
    }
  };

  const trySendResize = () => {
    const size = deps.readSize();
    if (!size) return;
    const { cols, rows } = size;
    if (!Number.isInteger(cols) || !Number.isInteger(rows) || cols <= 0 || rows <= 0) return;
    if (cols === lastSentCols && rows === lastSentRows) return;
    lastSentCols = cols;
    lastSentRows = rows;
    deps.sendResize(cols, rows);
  };

  const scheduleSettlingFit = () => {
    if (settlingFrameId) return;
    settlingFrameId = requestAnimationFrame(() => {
      settlingFrameId = 0;
      if (disposed) return;
      deps.fit();
    });
  };

  const scheduleInitialFit = () => {
    if (fitFrameId) return;
    fitFrameId = requestAnimationFrame(() => {
      fitFrameId = 0;
      if (disposed) return;
      deps.fit();
      scheduleSettlingFit();
    });
  };

  const scheduleResizeDebounce = () => {
    if (resizeTimer) clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => {
      resizeTimer = undefined;
      if (disposed) return;
      trySendResize();
    }, RESIZE_DEBOUNCE_MS);
  };

  return {
    requestRefit() {
      if (disposed) return;
      scheduleInitialFit();
      scheduleResizeDebounce();
    },
    noteReconnect() {
      lastSentCols = 0;
      lastSentRows = 0;
    },
    dispose() {
      disposed = true;
      cancelPending();
    },
  };
}
