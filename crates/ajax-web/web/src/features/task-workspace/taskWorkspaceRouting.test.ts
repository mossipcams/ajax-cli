import { describe, it, expect, afterEach } from "vitest";
import { sessionHash, taskHash } from "@/shared/lib/routes";
import type { BrowserCockpitView, BrowserTaskDetail } from "@/shared/lib/types";
import {
  clearTaskTerminalPreferred,
  writeTaskTerminalPreferred,
} from "./taskViewPreference";
import {
  cockpitSessionCapable,
  detailSessionCapable,
  openTaskWorkspaceHash,
  resolveTaskWorkspaceHash,
  shouldRedirectSessionToTerminal,
} from "./taskWorkspaceRouting";

const HANDLE = "web/fix-login";

function cockpit(
  cards: Array<{ qualified_handle: string; session_capable?: boolean }>,
): BrowserCockpitView {
  return {
    backend: { version: "test" },
    repos: { repos: [] },
    cards: cards.map((card, index) => ({
      id: `card-${index}`,
      repo: "web",
      title: "Task",
      status: "waiting",
      last_activity_unix_secs: 0,
      actions: [],
      ...card,
    })),
    inbox: { items: [] },
  } as BrowserCockpitView;
}

function detail(overrides: Partial<BrowserTaskDetail> = {}): BrowserTaskDetail {
  return {
    qualified_handle: HANDLE,
    repo: "web",
    title: "Fix login",
    branch: "ajax/fix-login",
    base_branch: "main",
    worktree_path: "/repo/web__worktrees/ajax-fix-login",
    tmux_session: "ajax-web-fix-login",
    lifecycle: "Reviewable",
    agent: "Codex",
    agent_status: "Idle",
    status: "waiting",
    status_explanation: "Ready for review",
    actions: [],
    live_status_kind: "WaitingForApproval",
    live_status_summary: "waiting",
    annotations: [],
    created_unix_secs: 0,
    last_activity_unix_secs: 0,
    agent_attempts: [],
    ...overrides,
  };
}

afterEach(() => {
  clearTaskTerminalPreferred(HANDLE);
  localStorage.clear();
});

describe("cockpitSessionCapable", () => {
  it("returns true when the cockpit card matches and is session-capable", () => {
    const view = cockpit([{ qualified_handle: HANDLE, session_capable: true }]);
    expect(cockpitSessionCapable(HANDLE, view)).toBe(true);
  });

  it("returns false when the card is not session-capable", () => {
    const view = cockpit([{ qualified_handle: HANDLE, session_capable: false }]);
    expect(cockpitSessionCapable(HANDLE, view)).toBe(false);
  });

  it("returns false when no card matches the handle", () => {
    const view = cockpit([{ qualified_handle: "web/other", session_capable: true }]);
    expect(cockpitSessionCapable(HANDLE, view)).toBe(false);
  });

  it("returns false when cockpit is missing", () => {
    expect(cockpitSessionCapable(HANDLE, null)).toBe(false);
    expect(cockpitSessionCapable(HANDLE, undefined)).toBe(false);
  });
});

describe("detailSessionCapable", () => {
  const view = cockpit([{ qualified_handle: HANDLE, session_capable: true }]);

  it("returns true when detail matches and cockpit reports session-capable", () => {
    expect(detailSessionCapable(detail(), HANDLE, view)).toBe(true);
  });

  it("returns false when detail is missing or handle mismatches", () => {
    expect(detailSessionCapable(null, HANDLE, view)).toBe(false);
    expect(detailSessionCapable(undefined, HANDLE, view)).toBe(false);
    expect(detailSessionCapable(detail({ qualified_handle: "web/other" }), HANDLE, view)).toBe(
      false,
    );
  });

  it("returns false when detail explicitly disables session capability", () => {
    expect(detailSessionCapable(detail({ session_capable: false }), HANDLE, view)).toBe(false);
  });

  it("returns false when cockpit does not report session capability", () => {
    const nonCapableView = cockpit([{ qualified_handle: HANDLE, session_capable: false }]);
    expect(detailSessionCapable(detail(), HANDLE, nonCapableView)).toBe(false);
  });
});

describe("resolveTaskWorkspaceHash", () => {
  it("defaults capable tasks to Chat when orchestration is on and Terminal is not preferred", () => {
    expect(
      resolveTaskWorkspaceHash(HANDLE, {
        orchestrationChat: true,
        sessionCapable: true,
        terminalPreferred: false,
      }),
    ).toBe(sessionHash(HANDLE));
  });

  it("opens Terminal when Terminal is preferred", () => {
    expect(
      resolveTaskWorkspaceHash(HANDLE, {
        orchestrationChat: true,
        sessionCapable: true,
        terminalPreferred: true,
      }),
    ).toBe(taskHash(HANDLE));
  });

  it("reads terminal preference from storage when terminalPreferred is omitted", () => {
    writeTaskTerminalPreferred(HANDLE);
    expect(
      resolveTaskWorkspaceHash(HANDLE, {
        orchestrationChat: true,
        sessionCapable: true,
      }),
    ).toBe(taskHash(HANDLE));
  });

  it("falls back to Terminal when the task is not session-capable", () => {
    expect(
      resolveTaskWorkspaceHash(HANDLE, {
        orchestrationChat: true,
        sessionCapable: false,
        terminalPreferred: false,
      }),
    ).toBe(taskHash(HANDLE));
  });

  it("falls back to Terminal when orchestration Chat is off", () => {
    expect(
      resolveTaskWorkspaceHash(HANDLE, {
        orchestrationChat: false,
        sessionCapable: true,
        terminalPreferred: false,
      }),
    ).toBe(taskHash(HANDLE));
  });
});

describe("shouldRedirectSessionToTerminal", () => {
  it("returns false when detail is missing or handle mismatches", () => {
    expect(shouldRedirectSessionToTerminal(HANDLE, null)).toBe(false);
    expect(shouldRedirectSessionToTerminal(HANDLE, detail({ qualified_handle: "web/other" }))).toBe(
      false,
    );
  });

  it("returns true when detail explicitly disables session capability", () => {
    expect(shouldRedirectSessionToTerminal(HANDLE, detail({ session_capable: false }))).toBe(true);
  });

  it("returns true when Terminal is preferred", () => {
    writeTaskTerminalPreferred(HANDLE);
    expect(shouldRedirectSessionToTerminal(HANDLE, detail())).toBe(true);
  });

  it("returns false for session-capable tasks without Terminal preference", () => {
    expect(shouldRedirectSessionToTerminal(HANDLE, detail())).toBe(false);
  });
});

describe("openTaskWorkspaceHash", () => {
  it("matches resolveTaskWorkspaceHash for Chat-default routing", () => {
    const options = { orchestrationChat: true, sessionCapable: true };
    expect(openTaskWorkspaceHash(HANDLE, options)).toBe(
      resolveTaskWorkspaceHash(HANDLE, options),
    );
    expect(openTaskWorkspaceHash(HANDLE, options)).toBe(sessionHash(HANDLE));
  });

  it("matches resolveTaskWorkspaceHash for non-capable fallback", () => {
    const options = { orchestrationChat: true, sessionCapable: false };
    expect(openTaskWorkspaceHash(HANDLE, options)).toBe(taskHash(HANDLE));
  });

  it("matches resolveTaskWorkspaceHash when orchestration Chat is off", () => {
    const options = { orchestrationChat: false, sessionCapable: true };
    expect(openTaskWorkspaceHash(HANDLE, options)).toBe(taskHash(HANDLE));
  });
});
