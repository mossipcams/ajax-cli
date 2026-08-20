import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { vi } from "vitest";
import { useEffect, useState, type ComponentProps } from "react";
import { render, fireEvent, screen, act } from "@testing-library/react";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";
import ChatSurface from "./ChatSurface";
import chatSurfaceSource from "./ChatSurface?raw";
import TaskDetailsSheet from "@/features/task-workspace/TaskDetailsSheet";
import TaskWorkspaceHeader from "@/features/task-workspace/TaskWorkspaceHeader";
import { ActionBar, visibleTaskActions } from "@/features/task/public";
import * as webSessionTransport from "@/shared/lib/webSessionTransport";
import taskDetail from "@/fixtures/task-detail.json";
import type { BrowserTaskDetail } from "@/shared/lib/types";

const here = dirname(fileURLToPath(import.meta.url));
export const stylesSource = readOrderedStylesSource(join(here, "../.."));
export { chatSurfaceSource };

export const transport = {
  // `WebSessionTransport.sendPrompt` returns the clientMessageId it queued, and
  // "" when it refuses to send; the composer keys off that.
  sendPrompt: vi.fn(() => "cmid-1"),
  sendCancel: vi.fn(),
  setModel: vi.fn(),
  respondPermission: vi.fn(),
  dispose: vi.fn(),
};

export const chatH = {
  emit: undefined as ((event: webSessionTransport.WebSessionServerEvent) => void) | undefined,
  ready: undefined as ((model: string) => void) | undefined,
  autoReady: true,
  frameQueue: [] as FrameRequestCallback[],
};

export function flushRaf() {
  act(() => {
    for (const callback of chatH.frameQueue.splice(0)) callback(0);
  });
}

export function stubSessionTransport() {
  vi.spyOn(webSessionTransport, "connectWebSessionTransport").mockImplementation(
    (_handle, callbacks) => {
      chatH.emit = callbacks.onEvent;
      chatH.ready = callbacks.onReady;
      if (chatH.autoReady) callbacks.onReady("auto");
      return transport;
    },
  );
}

export function openTaskDetails() {
  fireEvent.click(screen.getByTestId("session-details"));
}

export function openSwitchPanel() {
  fireEvent.click(screen.getByTestId("harness-swap-open"));
}

export function ChatWithSheet(
  props: Partial<ComponentProps<typeof ChatSurface>> & {
    onSwappedAgent?: () => void;
    onOpenTerminal?: () => void;
    onCockpit?: ComponentProps<typeof ActionBar>["onCockpit"];
    onResult?: ComponentProps<typeof ActionBar>["onResult"];
    onDismiss?: ComponentProps<typeof ActionBar>["onDismiss"];
    pendingConfirmAction?: string | null;
    onCancelPendingConfirm?: () => void;
  } = {},
) {
  const handle = props.handle ?? "web/fix-login";
  const detail = props.detail ?? (taskDetail as BrowserTaskDetail);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [sessionModel, setSessionModel] = useState<string | undefined>();
  const [sessionBusy, setSessionBusy] = useState(false);
  const panelId = "test-task-panel";

  useEffect(() => {
    if (props.pendingConfirmAction === "drop") setDetailsOpen(false);
  }, [props.pendingConfirmAction]);

  const actions = visibleTaskActions(detail.actions);
  const safeActions = actions.filter((action) => !action.destructive);
  const defaultHeadActions = safeActions.length ? (
    <div data-testid="session-head-actions">
      <ActionBar
        actions={safeActions}
        handle={detail.qualified_handle ?? handle}
        onCockpit={props.onCockpit}
        onResult={props.onResult}
        onMutated={props.onMutated}
        onDismiss={props.onDismiss}
        pendingConfirmAction={props.pendingConfirmAction}
        onCancelPendingConfirm={props.onCancelPendingConfirm}
      />
    </div>
  ) : null;

  return (
    <>
      <ChatSurface
        handle={handle}
        detail={detail}
        detailStatus={props.detailStatus ?? "ready"}
        headActions={props.headActions ?? defaultHeadActions}
        workspaceHeader={
          <TaskWorkspaceHeader
            detail={detail}
            handle={handle}
            onBack={props.onBack ?? (() => {})}
            onOpenDetails={() => setDetailsOpen(true)}
            detailsOpen={detailsOpen}
            detailsPanelId={panelId}
            detailsTestId="session-details"
          />
        }
        onSessionActivity={({ model, busy }) => {
          setSessionModel(model);
          setSessionBusy(busy);
        }}
        onBack={props.onBack}
        onOpenDiff={props.onOpenDiff}
        onMutated={props.onMutated}
      />
      <TaskDetailsSheet
        open={detailsOpen}
        onOpenChange={setDetailsOpen}
        panelId={panelId}
        mode="chat"
        detail={detail}
        sessionModel={sessionModel}
        harnessSwapDisabled={sessionBusy}
        onOpenTerminal={props.onOpenTerminal ?? (() => {})}
        onOpenDiff={props.onOpenDiff}
        onSwappedAgent={props.onSwappedAgent}
        onCockpit={props.onCockpit}
        onResult={props.onResult}
        onMutated={props.onMutated}
        onDismiss={props.onDismiss}
        pendingConfirmAction={props.pendingConfirmAction}
        onCancelPendingConfirm={props.onCancelPendingConfirm}
      />
    </>
  );
}

export function mountChat(overrides: Partial<ComponentProps<typeof ChatSurface>> = {}) {
  return render(<ChatWithSheet {...overrides} />);
}

export function send(event: webSessionTransport.WebSessionServerEvent) {
  act(() => chatH.emit?.(event));
  flushRaf();
}

/** Type into the composer and press Enter — send, queue, or stop-and-send,
 * whichever the current turn state makes it. */
export function typeComposer(text: string) {
  fireEvent.change(screen.getByLabelText("Message"), { target: { value: text } });
  fireEvent.keyDown(screen.getByLabelText("Message"), { key: "Enter", shiftKey: false });
}

export function prepareChatSurface() {
  chatH.emit = undefined;
  chatH.ready = undefined;
  chatH.autoReady = true;
  chatH.frameQueue = [];
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    chatH.frameQueue.push(callback);
    return chatH.frameQueue.length;
  });
  vi.stubGlobal("cancelAnimationFrame", () => {});
  transport.sendPrompt.mockClear();
  transport.setModel.mockClear();
  transport.respondPermission.mockClear();
  localStorage.clear();
  sessionStorage.clear();
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        models: [
          { id: "auto", label: "Auto" },
          { id: "composer-2.5", label: "Composer 2.5" },
        ],
      }),
    }),
  );
  stubSessionTransport();
}
