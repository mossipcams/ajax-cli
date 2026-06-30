import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import PanePanel from "./PanePanel.svelte";
import * as api from "../api";
import type { BrowserPaneState, BrowserTaskDetail } from "../types";

afterEach(() => vi.restoreAllMocks());

const detail: BrowserTaskDetail = {
  qualified_handle: "web/fix-login",
  repo: "web",
  title: "Fix login",
  branch: "b",
  base_branch: "main",
  worktree_path: "/w",
  tmux_session: "ajax-web-fix-login",
  lifecycle: "Active",
  agent: "Codex",
  agent_status: "Idle",
  status: "waiting",
  actions: [],
  annotations: [],
  created_unix_secs: 0,
  last_activity_unix_secs: 0,
  agent_attempts: [],
};

function snapshot(state: BrowserPaneState | null, tmux = true) {
  return { kind: "ok" as const, snapshot: { sequence: 1, lines: ["log line"], tmux_exists: tmux, state } };
}

describe("PanePanel", () => {
  it("shows an explicit message when tmux is missing", async () => {
    vi.spyOn(api, "fetchPane").mockResolvedValue(snapshot(null, false));
    const { findByText } = render(PanePanel, { props: { handle: "web/fix-login", detail } });
    expect(await findByText("Tmux session is missing. Sync the task to recover.")).toBeInTheDocument();
  });

  it("shows approve/deny only for an answerable fingerprinted prompt", async () => {
    vi.spyOn(api, "fetchPane").mockResolvedValue(
      snapshot({ kind: "WaitingForApproval", answerable: true, fingerprint: "fp", command: "rm -rf" }),
    );
    const { findByText } = render(PanePanel, { props: { handle: "web/fix-login", detail } });
    expect(await findByText("Approve")).toBeInTheDocument();
    expect(await findByText("Deny")).toBeInTheDocument();
  });

  it("hides answer buttons when the prompt is not answerable", async () => {
    vi.spyOn(api, "fetchPane").mockResolvedValue(
      snapshot({ kind: "WaitingForApproval", answerable: false, fingerprint: null }),
    );
    const { findByText, queryByText } = render(PanePanel, { props: { handle: "web/fix-login", detail } });
    expect(await findByText("Open the terminal below for this approval.")).toBeInTheDocument();
    expect(queryByText("Approve")).toBeNull();
  });

  it("maps a 409 answer conflict to the moved-on message", async () => {
    vi.spyOn(api, "fetchPane").mockResolvedValue(
      snapshot({ kind: "WaitingForApproval", answerable: true, fingerprint: "fp" }),
    );
    vi.spyOn(api, "postAnswer").mockRejectedValue(new api.ApiError("conflict", "409", 409));
    const onResult = vi.fn();
    const { findByText } = render(PanePanel, { props: { handle: "web/fix-login", detail, onResult } });
    await fireEvent.click(await findByText("Approve"));
    await vi.waitFor(() =>
      expect(onResult).toHaveBeenCalledWith(
        "The agent moved on before this approval was sent",
        null,
        true,
      ),
    );
  });

  it("shows a composer for WaitingForInput and sends free-form text", async () => {
    vi.spyOn(api, "fetchPane").mockResolvedValue(
      snapshot({ kind: "WaitingForInput", answerable: false, prompt: "What should I do next?" }),
    );
    const postTaskInput = vi.spyOn(api, "postTaskInput").mockResolvedValue({ sequence_hint: 2 });
    const onResult = vi.fn();
    const { findByPlaceholderText, findByText, getByText } = render(PanePanel, {
      props: { handle: "web/fix-login", detail, onResult },
    });

    const textarea = await findByPlaceholderText("Type a reply for the agent");
    await fireEvent.input(textarea, { target: { value: "write the tests" } });
    await fireEvent.click(getByText("Send"));

    await vi.waitFor(() =>
      expect(postTaskInput).toHaveBeenCalledWith("web/fix-login", {
        text: "write the tests",
        submit: true,
        request_id: expect.any(String),
      }),
    );
    expect(await findByText("Send")).toBeInTheDocument();
  });

  it("does not send whitespace-only free-form input", async () => {
    vi.spyOn(api, "fetchPane").mockResolvedValue(
      snapshot({ kind: "WaitingForInput", answerable: false, prompt: "What should I do next?" }),
    );
    const postTaskInput = vi.spyOn(api, "postTaskInput").mockResolvedValue({ sequence_hint: 2 });
    const { findByPlaceholderText, findByText } = render(PanePanel, {
      props: { handle: "web/fix-login", detail },
    });

    const textarea = await findByPlaceholderText("Type a reply for the agent");
    await fireEvent.input(textarea, { target: { value: "   " } });
    const send = await findByText("Send");
    expect(send).toBeDisabled();
    await fireEvent.click(send);
    expect(postTaskInput).not.toHaveBeenCalled();
  });

  it("maps a 429 input rate limit to the slow-down message", async () => {
    vi.spyOn(api, "fetchPane").mockResolvedValue(
      snapshot({ kind: "WaitingForInput", answerable: false, prompt: "What should I do next?" }),
    );
    vi.spyOn(api, "postTaskInput").mockRejectedValue(new api.ApiError("rate-limit", "429", 429));
    const onResult = vi.fn();
    const { findByPlaceholderText, getByText } = render(PanePanel, {
      props: { handle: "web/fix-login", detail, onResult },
    });

    const textarea = await findByPlaceholderText("Type a reply for the agent");
    await fireEvent.input(textarea, { target: { value: "continue" } });
    await fireEvent.click(getByText("Send"));

    await vi.waitFor(() =>
      expect(onResult).toHaveBeenCalledWith(
        "Slow down — too many actions in a short window",
        null,
        true,
      ),
    );
  });
});
