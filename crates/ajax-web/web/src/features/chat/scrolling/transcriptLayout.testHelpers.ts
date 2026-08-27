import { vi } from "vitest";
import {
  pinTranscriptToLiveEdge,
  transcriptAtLiveEdge,
  transcriptScrollBottom,
} from "@/shared/lib/sessionViewport";

/** No-op — live-edge assertions use scrollTop metrics in jsdom. */
export function syncTranscriptLayoutRects(_thread: HTMLDivElement): void {}

/** Legacy hook for tests that still patch scrollIntoView; pin uses scrollTop now. */
export function installTranscriptScrollIntoViewMock(): () => void {
  return () => {};
}

export function pinThreadToLiveEdge(thread: HTMLDivElement): void {
  pinTranscriptToLiveEdge(thread);
}

export function expectThreadAtLiveEdge(thread: HTMLDivElement): void {
  expect(transcriptAtLiveEdge(thread)).toBe(true);
}

export function expectThreadAwayFromLiveEdge(thread: HTMLDivElement): void {
  expect(transcriptAtLiveEdge(thread)).toBe(false);
}

/** Vitest helper: scroll away from live edge. */
export function scrollThreadToHistory(thread: HTMLDivElement, scrollTop: number): void {
  thread.scrollTop = scrollTop;
}

export function stubScrollMetrics(
  thread: HTMLDivElement,
  scrollHeight: number,
  clientHeight: number,
  scrollTop = 0,
): void {
  Object.defineProperty(thread, "scrollHeight", { configurable: true, value: scrollHeight });
  Object.defineProperty(thread, "clientHeight", { configurable: true, value: clientHeight });
  thread.scrollTop = scrollTop;
}

export function liveEdgeScrollTop(scrollHeight: number, clientHeight: number): number {
  return Math.max(0, scrollHeight - clientHeight);
}

export function mockScrollByToAdjustScrollTop(): () => void {
  const previous = HTMLElement.prototype.scrollBy;
  HTMLElement.prototype.scrollBy = function scrollBy(options?: ScrollToOptions | number) {
    const delta =
      typeof options === "number"
        ? options
        : typeof options?.top === "number"
          ? options.top
          : 0;
    if (this.classList.contains("session-thread")) {
      this.scrollTop += delta;
    }
  };
  return () => {
    HTMLElement.prototype.scrollBy = previous;
  };
}

export function mockPinSpy() {
  return vi.spyOn({ pinTranscriptToLiveEdge }, "pinTranscriptToLiveEdge");
}

export function expectScrollBottomNear(
  thread: HTMLDivElement,
  threshold = 16,
): void {
  expect(
    transcriptScrollBottom(thread.scrollTop, thread.scrollHeight, thread.clientHeight),
  ).toBeLessThanOrEqual(threshold);
}
