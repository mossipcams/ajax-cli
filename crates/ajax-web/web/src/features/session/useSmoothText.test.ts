import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act } from "@testing-library/react";
import { createElement, useState } from "react";
import { useSmoothText } from "./useSmoothText";

type FrameCallback = (time: number) => void;

let frameQueue: FrameCallback[] = [];
let nextFrameId = 1;
const frameCallbacks = new Map<number, FrameCallback>();
let frameTime = 0;

function flushFrames(untilMs: number, stepMs = 16): void {
  while (frameTime < untilMs || frameQueue.length > 0) {
    if (frameQueue.length === 0) {
      frameTime += stepMs;
      continue;
    }
    frameTime += stepMs;
    const callbacks = frameQueue.splice(0);
    frameCallbacks.clear();
    for (const callback of callbacks) {
      callback(frameTime);
    }
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

function Harness({
  text,
  enabled,
}: {
  text: string;
  enabled: boolean;
}) {
  const shown = useSmoothText(text, enabled);
  return createElement("div", { "data-testid": "shown" }, shown);
}

function ToggleHarness({ text }: { text: string }) {
  const [enabled, setEnabled] = useState(true);
  const shown = useSmoothText(text, enabled);
  return createElement(
    "div",
    null,
    createElement("div", { "data-testid": "shown" }, shown),
    createElement(
      "button",
      { type: "button", onClick: () => setEnabled(false) },
      "settle",
    ),
  );
}

beforeEach(() => {
  frameQueue = [];
  nextFrameId = 1;
  frameCallbacks.clear();
  frameTime = 0;
  stubAnimationFrames();
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: false }));
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("useSmoothText", () => {
  it("reveals a long string over ~250ms when enabled", () => {
    const text = "x".repeat(100);
    render(createElement(Harness, { text, enabled: true }));
    const node = screen.getByTestId("shown");
    expect(node.textContent!.length).toBeLessThan(text.length);

    act(() => {
      flushFrames(300);
    });
    expect(node).toHaveTextContent(text);
  });

  it("shows the full string immediately when disabled", () => {
    const text = "Complete on first paint";
    render(createElement(Harness, { text, enabled: false }));
    expect(screen.getByTestId("shown")).toHaveTextContent(text);
  });

  it("shows the full string immediately when prefers-reduced-motion", () => {
    vi.stubGlobal(
      "matchMedia",
      vi.fn().mockReturnValue({ matches: true }),
    );
    const text = "No animation please";
    render(createElement(Harness, { text, enabled: true }));
    expect(screen.getByTestId("shown")).toHaveTextContent(text);
  });

  it("keeps shown as a prefix while text grows and eventually catches up", () => {
    const { rerender } = render(createElement(Harness, { text: "Hel", enabled: true }));
    const node = screen.getByTestId("shown");

    act(() => {
      flushFrames(50);
    });
    expect("Hel".startsWith(node.textContent ?? "")).toBe(true);

    rerender(createElement(Harness, { text: "Hello", enabled: true }));
    act(() => {
      flushFrames(300);
    });
    expect(node).toHaveTextContent("Hello");
  });

  it("snaps to the full string when enabled becomes false", () => {
    const text = "x".repeat(80);
    render(createElement(ToggleHarness, { text }));
    const node = screen.getByTestId("shown");
    expect(node.textContent!.length).toBeLessThan(text.length);

    act(() => {
      screen.getByRole("button", { name: "settle" }).click();
    });
    expect(node).toHaveTextContent(text);
  });
});
