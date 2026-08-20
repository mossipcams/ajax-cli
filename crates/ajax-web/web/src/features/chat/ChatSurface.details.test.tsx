import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { screen, act } from "@testing-library/react";
import taskDetail from "@/fixtures/task-detail.json";
import type { BrowserTaskDetail } from "@/shared/lib/types";
import {
  chatH,
  chatSurfaceSource,
  mountChat,
  openSwitchPanel,
  openTaskDetails,
  prepareChatSurface,
  emitConnectedSnapshot,
  stylesSource,
} from "./ChatSurface.testHarness";

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
  sessionStorage.clear();
});

describe("ChatSurface task details polish", () => {
  beforeEach(() => {
    prepareChatSurface();
  });

  it("styles sheet field labels with tracked uppercase chrome", () => {
    const labelCss =
      stylesSource.match(/\.session-details-sheet \.field-label\s*\{([^}]*)\}/)?.[1] ?? "";
    expect(labelCss).toMatch(/text-transform:\s*uppercase/);
    expect(labelCss).toMatch(/letter-spacing:\s*var\(--tracking-label\)/);
    expect(labelCss).toMatch(/color:\s*var\(--ink-muted\)/);
  });

  it("lifts Close to 44px in the task details sheet", () => {
    const closeCss =
      stylesSource.match(
        /\.session-details-sheet \.session-sheet-header \.pill[\s\S]*?\{([^}]*)\}/,
      )?.[1] ?? "";
    expect(closeCss).toMatch(/min-height:\s*44px/);
  });

  it("exposes aria-expanded on Details and Switch", async () => {
    chatH.autoReady = false;
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" } });
    emitConnectedSnapshot("composer-2.5", [
      {
        id: "model",
        category: "model",
        name: "Model",
        type: "select",
        currentValue: "composer-2.5",
        choices: [
          { value: "composer-2.5", name: "Composer 2.5" },
          { value: "auto", name: "Auto" },
        ],
      },
    ]);

    const details = screen.getByTestId("session-details");
    expect(details).toHaveAttribute("aria-expanded", "false");
    expect(details).toHaveAttribute("aria-controls");

    openTaskDetails();
    expect(details).toHaveAttribute("aria-expanded", "true");

    expect(screen.getByTestId("harness-swap")).not.toHaveClass("is-open");
    openSwitchPanel();
    expect(screen.getByTestId("harness-swap")).toHaveClass("is-open");
    expect(await screen.findByTestId("harness-swap-harness-only")).toBeInTheDocument();
  });

  it("pins observation error under identity with the task-detail prefix", () => {
    mountChat({
      detail: {
        ...(taskDetail as BrowserTaskDetail),
        runtime_observation_error: "tmux session missing",
      },
    });
    openTaskDetails();

    const identity = screen.getByTestId("session-task-identity");
    const observationError = screen.getByTestId("session-observation-error");
    const harnessSwap = screen.getByTestId("harness-swap");
    expect(observationError).toHaveTextContent("Observation error: tmux session missing");
    expect(identity.compareDocumentPosition(observationError) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(observationError.compareDocumentPosition(harnessSwap) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("does not render a separate in-session model picker (#979)", () => {
    mountChat({ detail: { ...(taskDetail as BrowserTaskDetail), agent: "cursor" } });
    openTaskDetails();
    expect(screen.queryByTestId("session-model-select")).not.toBeInTheDocument();
    expect(screen.getByTestId("harness-swap")).toBeInTheDocument();
  });

  it("does not give the first sheet ActionBar action primary fill", () => {
    const mutedCss =
      stylesSource.match(/\.session-sheet-actions-muted \.action\.primary\s*\{([^}]*)\}/)?.[1] ??
      "";
    expect(mutedCss).toMatch(/background:\s*transparent/);
    expect(mutedCss).not.toMatch(/background:\s*var\(--accent\)/);
  });
});

describe("ChatSurface ownership", () => {
  it("does not import task UI internals", () => {
    expect(chatSurfaceSource).not.toMatch(/@\/features\/task\/ActionBar/);
    expect(chatSurfaceSource).not.toMatch(/@\/features\/task\/HarnessSwap/);
    expect(chatSurfaceSource).not.toMatch(/@\/features\/task\/TaskMetaDetails/);
    expect(chatSurfaceSource).not.toMatch(/@\/features\/task\/TaskLoadError/);
    expect(chatSurfaceSource).not.toMatch(/visibleTaskActions/);
  });

  it("does not import terminal speech or xterm types", () => {
    expect(chatSurfaceSource).not.toMatch(/@\/features\/terminal\/useTaskTerminalSpeech/);
    expect(chatSurfaceSource).not.toMatch(/@xterm\/xterm/);
    expect(chatSurfaceSource).not.toMatch(/terminalConnection/);
    expect(chatSurfaceSource).not.toMatch(/useSessionChatViewport/);
    expect(chatSurfaceSource).toMatch(/\.\/speech\/useChatSpeech/);
    expect(chatSurfaceSource).toMatch(/\.\/viewport\/useChatViewport/);
  });
});
