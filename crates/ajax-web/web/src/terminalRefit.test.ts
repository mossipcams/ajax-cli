import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createRefitController } from "./terminalRefit";

type FrameCallback = (time: number) => void;

let frameQueue: FrameCallback[] = [];
let nextFrameId = 1;
const frameCallbacks = new Map<number, FrameCallback>();

function flushFrame(): void {
  const callbacks = frameQueue.splice(0);
  frameCallbacks.clear();
  for (const callback of callbacks) {
    callback(0);
  }
}

function stubAnimationFrames(): void {
  vi.stubGlobal("requestAnimationFrame", (callback: FrameCallback) => {
    const id = nextFrameId++;
    frameCallbacks.set(id, callback);
    frameQueue.push(callback);
    return id;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => {
    const callback = frameCallbacks.get(id);
    if (!callback) return;
    frameCallbacks.delete(id);
    const index = frameQueue.indexOf(callback);
    if (index >= 0) frameQueue.splice(index, 1);
  });
}

function createController(
  readSize: () => { cols: number; rows: number } | null = () => ({ cols: 87, rows: 24 }),
) {
  const fit = vi.fn();
  const sendResize = vi.fn();
  const controller = createRefitController({ fit, readSize, sendResize });
  return { controller, fit, sendResize, readSize };
}

/** Run frame settling and let the PTY debounce window complete. */
function settleBurst(controller: { requestRefit: () => void }): void {
  controller.requestRefit();
  flushFrame();
  flushFrame();
  vi.advanceTimersByTime(100);
}

beforeEach(() => {
  frameQueue = [];
  nextFrameId = 1;
  frameCallbacks.clear();
  stubAnimationFrames();
  // Fake only the timeout APIs: the default useFakeTimers() also fakes
  // requestAnimationFrame, which would silently replace the manual frame
  // queue above and starve flushFrame().
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("createRefitController", () => {
  it("coalesces same-frame requestRefit bursts into one fit", () => {
    const { controller, fit } = createController();

    controller.requestRefit();
    controller.requestRefit();
    controller.requestRefit();
    flushFrame();

    expect(fit).toHaveBeenCalledTimes(1);
  });

  it("fits on the next frame and one follow-up frame, then stops", () => {
    const { controller, fit } = createController();

    controller.requestRefit();
    flushFrame();
    flushFrame();
    flushFrame();

    expect(fit).toHaveBeenCalledTimes(2);
  });

  it("debounces PTY resize 100 ms after the last requestRefit", () => {
    const { controller, sendResize } = createController();

    controller.requestRefit();
    vi.advanceTimersByTime(99);
    expect(sendResize).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(sendResize).toHaveBeenCalledTimes(1);
    expect(sendResize).toHaveBeenCalledWith(87, 24);

    const second = createController();
    second.controller.requestRefit();
    vi.advanceTimersByTime(50);
    second.controller.requestRefit();
    vi.advanceTimersByTime(99);
    expect(second.sendResize).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(second.sendResize).toHaveBeenCalledTimes(1);
    expect(second.sendResize).toHaveBeenCalledWith(87, 24);
  });

  it("deduplicates adjacent identical sizes across bursts", () => {
    const { controller, sendResize } = createController();

    settleBurst(controller);
    expect(sendResize).toHaveBeenCalledTimes(1);
    expect(sendResize).toHaveBeenCalledWith(87, 24);

    settleBurst(controller);
    expect(sendResize).toHaveBeenCalledTimes(1);

    const readSize = vi.fn(() => ({ cols: 100, rows: 30 }));
    const fit = vi.fn();
    const dedupeController = createRefitController({ fit, readSize, sendResize });
    settleBurst(dedupeController);
    expect(sendResize).toHaveBeenCalledTimes(2);
    expect(sendResize).toHaveBeenLastCalledWith(100, 30);
  });

  it("never sends invalid or non-integer sizes", () => {
    const invalidSizes: Array<{ cols: number; rows: number } | null> = [
      null,
      { cols: 0, rows: 24 },
      { cols: 87.5, rows: 24 },
    ];

    for (const size of invalidSizes) {
      const fit = vi.fn();
      const sendResize = vi.fn();
      const readSize = vi.fn(() => size);
      const controller = createRefitController({ fit, readSize, sendResize });

      settleBurst(controller);
      expect(sendResize).not.toHaveBeenCalled();
    }
  });

  it("clears dedupe memory on reconnect so the same size can be sent again", () => {
    const { controller, sendResize } = createController();

    settleBurst(controller);
    expect(sendResize).toHaveBeenCalledTimes(1);
    expect(sendResize).toHaveBeenCalledWith(87, 24);

    controller.noteReconnect();
    settleBurst(controller);
    expect(sendResize).toHaveBeenCalledTimes(2);
    expect(sendResize).toHaveBeenLastCalledWith(87, 24);
  });

  it("cancels pending work on dispose and ignores later requests", () => {
    const { controller, fit, sendResize } = createController();

    controller.requestRefit();
    controller.dispose();

    flushFrame();
    flushFrame();
    vi.advanceTimersByTime(100);
    expect(fit).not.toHaveBeenCalled();
    expect(sendResize).not.toHaveBeenCalled();

    controller.requestRefit();
    flushFrame();
    flushFrame();
    vi.advanceTimersByTime(100);
    expect(fit).not.toHaveBeenCalled();
    expect(sendResize).not.toHaveBeenCalled();
  });
});
