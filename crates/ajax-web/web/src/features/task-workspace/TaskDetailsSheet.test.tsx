import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { render, fireEvent, screen, waitFor, within } from "@testing-library/react";
import TaskDetailsSheet from "./TaskDetailsSheet";
import taskDetail from "@/fixtures/task-detail.json";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import { readOrderedStylesSource } from "@/shared/lib/styleSources";
import * as api from "@/shared/lib/api";

const here = dirname(fileURLToPath(import.meta.url));
const stylesSource = readOrderedStylesSource(join(here, "../.."));

function detail(overrides: Partial<BrowserTaskDetail> = {}): BrowserTaskDetail {
  return { ...(taskDetail as BrowserTaskDetail), ...overrides };
}

describe("TaskDetailsSheet chat entry", () => {
  beforeEach(() => {
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
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("opens from chat mode with Ajax terminal and diff primary tools", () => {
    const onOpenTerminal = vi.fn();
    render(
      <TaskDetailsSheet
        open
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="chat"
        detail={detail()}
        onOpenTerminal={onOpenTerminal}
        onOpenDiff={vi.fn()}
      />,
    );

    const sheet = screen.getByTestId("task-details-sheet");
    expect(within(sheet).getByTestId("session-primary-tools")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("session-ajax-terminal"));
    expect(onOpenTerminal).toHaveBeenCalledOnce();
  });

  it("closes the sheet before opening the diff from chat mode", () => {
    const calls: string[] = [];
    const onOpenChange = vi.fn(() => calls.push("close"));
    const onOpenDiff = vi.fn(() => calls.push("diff"));
    render(
      <TaskDetailsSheet
        open
        onOpenChange={onOpenChange}
        panelId="task-panel"
        mode="chat"
        detail={detail()}
        onOpenTerminal={vi.fn()}
        onOpenDiff={onOpenDiff}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Show diff" }));

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onOpenDiff).toHaveBeenCalledOnce();
    expect(calls).toEqual(["close", "diff"]);
  });

  it("leads the sheet with task identity in chat mode", () => {
    render(
      <TaskDetailsSheet
        open
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="chat"
        detail={detail()}
      />,
    );

    const sheet = screen.getByTestId("task-details-sheet");
    const identity = screen.getByTestId("session-task-identity");
    expect(identity).toHaveTextContent("Fix login");
    expect(identity).toHaveTextContent("web/fix-login");
    expect(within(sheet).getByTestId("session-details-body")).toContainElement(identity);
  });

  it("shows harness swap when the task has an agent", () => {
    render(
      <TaskDetailsSheet
        open
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="chat"
        detail={detail({ agent: "cursor" })}
      />,
    );

    expect(screen.getByTestId("harness-swap")).toBeInTheDocument();
  });

  it("calls onSwappedAgent and onMutated after a successful harness swap", async () => {
    vi.spyOn(api, "swapTaskAgent").mockResolvedValue({ ok: true, response: {} });
    const onSwappedAgent = vi.fn();
    const onMutated = vi.fn();
    render(
      <TaskDetailsSheet
        open
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="chat"
        detail={detail({ agent: "cursor" })}
        onSwappedAgent={onSwappedAgent}
        onMutated={onMutated}
      />,
    );

    fireEvent.click(screen.getByTestId("harness-swap-open"));
    fireEvent.click(screen.getByRole("radio", { name: "Codex" }));
    fireEvent.click(screen.getByTestId("harness-swap-apply"));

    await waitFor(() => expect(onSwappedAgent).toHaveBeenCalledOnce());
    expect(onMutated).toHaveBeenCalledOnce();
  });
});

describe("TaskDetailsSheet restart agent action", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          models: [{ id: "auto", label: "Auto" }],
        }),
      }),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("pins Restart agent as a secondary tool in chat mode without a quick-actions bar", () => {
    render(
      <TaskDetailsSheet
        open
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="chat"
        detail={detail({ actions: [] })}
      />,
    );

    const sheet = screen.getByTestId("task-details-sheet");
    const primaryTools = within(sheet).getByTestId("session-primary-tools");
    expect(
      within(primaryTools).getByRole("button", { name: "Restart agent" }),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("session-quick-actions")).not.toBeInTheDocument();
  });

  it("shows Restart agent once in primary tools next to other actions in quick actions", () => {
    render(
      <TaskDetailsSheet
        open
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="chat"
        detail={detail({
          actions: [
            {
              action: "fix-ci",
              label: "Fix CI",
              destructive: false,
              confirmation_required: false,
            },
          ],
        })}
      />,
    );

    const primaryTools = screen.getByTestId("session-primary-tools");
    expect(within(primaryTools).getAllByRole("button", { name: "Restart agent" })).toHaveLength(1);
    const quickActions = screen.getByTestId("session-quick-actions");
    expect(within(quickActions).getByRole("button", { name: "Fix CI" })).toBeInTheDocument();
  });

  it("asks for one confirmation, then sends the WebSocket restart without posting an operation", () => {
    const postOperation = vi.spyOn(api, "postOperation");
    const onRestartAgent = vi.fn(() => true);
    const onOpenChange = vi.fn();
    const onResult = vi.fn();
    render(
      <TaskDetailsSheet
        open
        onOpenChange={onOpenChange}
        panelId="task-panel"
        mode="chat"
        detail={detail({ actions: [] })}
        onRestartAgent={onRestartAgent}
        onResult={onResult}
      />,
    );

    fireEvent.click(screen.getByTestId("session-restart-agent"));
    expect(onRestartAgent).not.toHaveBeenCalled();
    expect(screen.getByTestId("session-restart-agent")).toHaveTextContent(
      "Confirm restart agent",
    );

    fireEvent.click(screen.getByTestId("session-restart-agent"));
    expect(onRestartAgent).toHaveBeenCalledOnce();
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onResult).not.toHaveBeenCalled();
    expect(postOperation).not.toHaveBeenCalled();
  });

  it("closes the sheet and reports an error when Restart agent cannot run disconnected", () => {
    const onRestartAgent = vi.fn(() => false);
    const onOpenChange = vi.fn();
    const onResult = vi.fn();
    render(
      <TaskDetailsSheet
        open
        onOpenChange={onOpenChange}
        panelId="task-panel"
        mode="chat"
        detail={detail({ actions: [] })}
        onRestartAgent={onRestartAgent}
        onResult={onResult}
      />,
    );

    fireEvent.click(screen.getByTestId("session-restart-agent"));
    fireEvent.click(screen.getByTestId("session-restart-agent"));

    expect(onRestartAgent).toHaveBeenCalledOnce();
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onResult).toHaveBeenCalledWith(
      "Not connected. Try again once the session reconnects.",
      null,
      true,
    );
    expect(screen.getByTestId("session-restart-agent")).toHaveTextContent("Restart agent");
  });

  it("reports not connected and closes when Restart agent has no session to send to", () => {
    const onOpenChange = vi.fn();
    const onResult = vi.fn();
    render(
      <TaskDetailsSheet
        open
        onOpenChange={onOpenChange}
        panelId="task-panel"
        mode="chat"
        detail={detail({ actions: [] })}
        onResult={onResult}
      />,
    );

    fireEvent.click(screen.getByTestId("session-restart-agent"));
    fireEvent.click(screen.getByTestId("session-restart-agent"));

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onResult).toHaveBeenCalledWith(
      "Not connected. Try again once the session reconnects.",
      null,
      true,
    );
  });

  it("shows Restart agent in chat mode while idle when Core does not offer it", () => {
    render(
      <TaskDetailsSheet
        open
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="chat"
        detail={detail({ actions: [] })}
      />,
    );

    const primaryTools = screen.getByTestId("session-primary-tools");
    expect(within(primaryTools).getByRole("button", { name: "Restart agent" })).toBeInTheDocument();
    expect(screen.getByTestId("session-fresh-session")).toBeInTheDocument();
  });

  it("keeps Restart agent in chat mode regardless of session activity", () => {
    render(
      <TaskDetailsSheet
        open
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="chat"
        detail={detail({ actions: [] })}
      />,
    );

    const primaryTools = screen.getByTestId("session-primary-tools");
    expect(within(primaryTools).getByTestId("session-restart-agent")).toBeInTheDocument();
  });

  it("runs Fresh session only after a second tap", () => {
    const onClearSession = vi.fn(() => true);
    const onOpenChange = vi.fn();
    render(
      <TaskDetailsSheet
        open
        onOpenChange={onOpenChange}
        panelId="task-panel"
        mode="chat"
        detail={detail({ actions: [] })}
        onClearSession={onClearSession}
      />,
    );

    fireEvent.click(screen.getByTestId("session-fresh-session"));
    expect(onClearSession).not.toHaveBeenCalled();
    expect(onOpenChange).not.toHaveBeenCalled();
    const armed = screen.getByTestId("session-fresh-session");
    expect(armed).toHaveTextContent("Confirm fresh session");

    fireEvent.click(armed);
    expect(onClearSession).toHaveBeenCalledOnce();
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("closes the sheet and reports an error when Fresh session cannot run disconnected", () => {
    const onClearSession = vi.fn(() => false);
    const onOpenChange = vi.fn();
    const onResult = vi.fn();
    render(
      <TaskDetailsSheet
        open
        onOpenChange={onOpenChange}
        panelId="task-panel"
        mode="chat"
        detail={detail({ actions: [] })}
        onClearSession={onClearSession}
        onResult={onResult}
      />,
    );

    fireEvent.click(screen.getByTestId("session-fresh-session"));
    fireEvent.click(screen.getByTestId("session-fresh-session"));

    expect(onClearSession).toHaveBeenCalledOnce();
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onResult).toHaveBeenCalledWith(
      "Not connected. Try again once the session reconnects.",
      null,
      true,
    );
    expect(screen.getByTestId("session-fresh-session")).toHaveTextContent("Fresh session");
  });

  it("resets the Fresh session armed state when the sheet closes", () => {
    const onClearSession = vi.fn(() => true);
    const props = {
      onOpenChange: vi.fn(),
      panelId: "task-panel",
      mode: "chat" as const,
      detail: detail({ actions: [] }),
      onClearSession,
    };
    const { rerender } = render(<TaskDetailsSheet open {...props} />);

    fireEvent.click(screen.getByTestId("session-fresh-session"));
    expect(screen.getByTestId("session-fresh-session")).toHaveTextContent(
      "Confirm fresh session",
    );

    rerender(<TaskDetailsSheet open={false} {...props} />);
    rerender(<TaskDetailsSheet open {...props} />);

    expect(screen.getByTestId("session-fresh-session")).toHaveTextContent("Fresh session");
    expect(onClearSession).not.toHaveBeenCalled();
  });

  it("does not add Restart agent in terminal mode", () => {
    render(
      <TaskDetailsSheet
        open
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="terminal"
        detail={detail({ actions: [] })}
      />,
    );

    expect(screen.queryByTestId("session-quick-actions")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Restart agent" }),
    ).not.toBeInTheDocument();
  });
});

describe("TaskDetailsSheet terminal entry", () => {
  it("opens from terminal mode with Ajax chat when orchestration chat is enabled", () => {
    const onOpenChat = vi.fn();
    render(
      <TaskDetailsSheet
        open
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="terminal"
        detail={detail({ session_capable: true })}
        orchestrationChat
        onOpenChat={onOpenChat}
      />,
    );

    const sheet = screen.getByTestId("task-details-sheet");
    const primaryTools = within(sheet).getByTestId("task-primary-tools");
    fireEvent.click(within(primaryTools).getByRole("button", { name: "Ajax chat" }));
    expect(onOpenChat).toHaveBeenCalledOnce();
  });

  it("hides Ajax chat when orchestration chat is off", () => {
    render(
      <TaskDetailsSheet
        open
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="terminal"
        detail={detail({ session_capable: true })}
        orchestrationChat={false}
        onOpenChat={vi.fn()}
      />,
    );
    expect(screen.queryByRole("button", { name: "Ajax chat" })).not.toBeInTheDocument();
  });

  it("shows Ajax chat for acp-capable tasks before provision (#1092)", () => {
    const onOpenChat = vi.fn();
    render(
      <TaskDetailsSheet
        open
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="terminal"
        detail={detail({ session_capable: false, agent: "Codex" })}
        orchestrationChat
        onOpenChat={onOpenChat}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Ajax chat" }));
    expect(onOpenChat).toHaveBeenCalledOnce();
  });

  it("hides Ajax chat when the task agent has no ACP entry point", () => {
    render(
      <TaskDetailsSheet
        open
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="terminal"
        detail={detail({ session_capable: false, agent: "Other" })}
        orchestrationChat
        onOpenChat={vi.fn()}
      />,
    );
    expect(screen.queryByRole("button", { name: "Ajax chat" })).not.toBeInTheDocument();
  });

  it("does not show harness swap on the terminal sheet", () => {
    render(
      <TaskDetailsSheet
        open
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="terminal"
        detail={detail({ agent: "cursor" })}
      />,
    );
    expect(screen.queryByTestId("harness-swap")).not.toBeInTheDocument();
  });
});

describe("TaskDetailsSheet polish", () => {
  it("bounds session details sheets with a contained scroller for iOS-safe reachability", () => {
    const scrimBlock =
      stylesSource.match(/\.session-sheet-scrim\s*\{([^}]*)\}/)?.[1] ?? "";
    const sheetBlock =
      stylesSource.match(/\.session-details-sheet\s*\{([^}]*)\}/)?.[1] ?? "";
    const bodyBlock =
      stylesSource.match(/\.session-details-body\s*\{([^}]*)\}/)?.[1] ?? "";

    expect(scrimBlock).toMatch(/overflow:\s*hidden/);
    expect(sheetBlock).toMatch(/max-height:\s*100%/);
    expect(bodyBlock).toMatch(/overflow-y:\s*auto/);
  });

  it("closes when open becomes false", () => {
    const { rerender } = render(
      <TaskDetailsSheet
        open
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="chat"
        detail={detail()}
      />,
    );
    expect(screen.getByTestId("task-details-sheet")).toBeInTheDocument();

    rerender(
      <TaskDetailsSheet
        open={false}
        onOpenChange={vi.fn()}
        panelId="task-panel"
        mode="chat"
        detail={detail()}
      />,
    );
    expect(screen.queryByTestId("task-details-sheet")).not.toBeInTheDocument();
  });

  it("closes when Drop confirm arms via parent onOpenChange", () => {
    const onOpenChange = vi.fn();
    const { rerender } = render(
      <TaskDetailsSheet
        open
        onOpenChange={onOpenChange}
        panelId="task-panel"
        mode="chat"
        detail={detail()}
        pendingConfirmAction={null}
      />,
    );
    expect(screen.getByTestId("task-details-sheet")).toBeInTheDocument();

    rerender(
      <TaskDetailsSheet
        open={false}
        onOpenChange={onOpenChange}
        panelId="task-panel"
        mode="chat"
        detail={detail()}
        pendingConfirmAction="drop"
      />,
    );
    expect(screen.queryByTestId("task-details-sheet")).not.toBeInTheDocument();
  });
});
