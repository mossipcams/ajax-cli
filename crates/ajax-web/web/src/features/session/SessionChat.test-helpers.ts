import { createElement } from "react";
import { vi, afterEach } from "vitest";
import { render, act } from "@testing-library/react";
import SessionChat from "./SessionChat";
import * as webSessionTransport from "@/shared/lib/webSessionTransport";
import taskDetail from "@/fixtures/task-detail.json";
import type { BrowserTaskDetail } from "@/shared/lib/types";

export const transport = {
  sendPrompt: vi.fn(),
  sendCancel: vi.fn(),
  setModel: vi.fn(),
  respondPermission: vi.fn(),
  dispose: vi.fn(),
};

export let emit: ((event: webSessionTransport.WebSessionServerEvent) => void) | undefined;
export let signalReady: ((model?: string) => void) | undefined;
export let closeSocket: (() => void) | undefined;

export function resetSessionTransportState() {
  emit = undefined;
  signalReady = undefined;
  closeSocket = undefined;
  transport.sendPrompt.mockClear();
  transport.sendCancel.mockClear();
  transport.setModel.mockClear();
  transport.respondPermission.mockClear();
  transport.dispose.mockClear();
}

export function stubSessionTransport(options: { autoReady?: boolean } = { autoReady: true }) {
  resetSessionTransportState();
  vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
    (_handle, callbacks) => {
      emit = callbacks.onEvent;
      signalReady = (model = "auto") => callbacks.onReady(model);
      closeSocket = callbacks.onClosed;
      if (options.autoReady !== false) {
        callbacks.onReady("auto");
      }
      return transport;
    },
  );
}

export function mountChat(overrides: Partial<React.ComponentProps<typeof SessionChat>> = {}) {
  return render(
    createElement(SessionChat, {
      handle: "web/fix-login",
      detail: taskDetail as BrowserTaskDetail,
      detailStatus: "ready",
      ...overrides,
    }),
  );
}

export function send(event: webSessionTransport.WebSessionServerEvent) {
  act(() => emit?.(event));
}

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  Object.defineProperty(document, "visibilityState", {
    value: "visible",
    configurable: true,
  });
  localStorage.clear();
  sessionStorage.clear();
});
