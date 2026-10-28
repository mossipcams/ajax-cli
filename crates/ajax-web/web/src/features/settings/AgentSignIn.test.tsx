import { describe, it, expect, vi, afterEach } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import * as api from "@/shared/lib/api";
import AgentSignIn from "./AgentSignIn";

function notSignedInCursor() {
  return { present: false, valid: null, email: null, expiresAtMs: null, error: null };
}

function mockAgents(harnesses: api.AgentAuthStatus[]) {
  return vi.spyOn(api, "fetchAgentAuthStatus").mockResolvedValue({ harnesses });
}

function mockCursor() {
  vi.spyOn(api, "fetchCursorAuthStatus").mockResolvedValue(notSignedInCursor());
  vi.spyOn(api, "fetchCursorLoginPhase").mockResolvedValue({ phase: "idle" });
}

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("AgentSignIn", () => {
  it("renders one row per harness in server order", async () => {
    mockCursor();
    mockAgents([
      { harness: "claude", installed: true, signedIn: true, detail: null },
      { harness: "codex", installed: true, signedIn: false, detail: "Run `codex login` on the host" },
      { harness: "cursor", installed: true, signedIn: null, detail: "Could not determine sign-in status" },
      { harness: "pi", installed: false, signedIn: null, detail: "pi is not installed on the host" },
    ]);

    render(<AgentSignIn />);

    const section = await screen.findByTestId("agent-signin-settings");
    expect(within(section).getByRole("heading", { name: "Agents" })).toBeInTheDocument();
    expect(within(section).getAllByRole("term").map((term) => term.textContent)).toEqual([
      "Claude",
      "Codex",
      "Cursor",
      "Pi",
    ]);
  });

  it("renders all harness rows inside one status list", async () => {
    mockCursor();
    mockAgents([
      { harness: "claude", installed: true, signedIn: true, detail: null },
      { harness: "codex", installed: true, signedIn: false, detail: "Run `codex login` on the host" },
    ]);

    render(<AgentSignIn />);

    const list = await screen.findByTestId("agent-signin-list");
    expect(within(list).getByTestId("agent-signin-claude")).toBeInTheDocument();
    expect(within(list).getByTestId("agent-signin-codex")).toBeInTheDocument();
    expect(within(list).getAllByRole("term")).toHaveLength(2);
  });

  it("maps signedIn and installed to the four state labels", async () => {
    mockCursor();
    mockAgents([
      { harness: "claude", installed: true, signedIn: true, detail: null },
      { harness: "codex", installed: true, signedIn: false, detail: null },
      { harness: "cursor", installed: true, signedIn: null, detail: null },
      { harness: "pi", installed: false, signedIn: false, detail: null },
    ]);

    render(<AgentSignIn />);

    expect(await screen.findByText("Signed in")).toBeInTheDocument();
    expect(screen.getByText("Signed out")).toBeInTheDocument();
    expect(screen.getByText("Unknown")).toBeInTheDocument();
    expect(screen.getByText("Not installed")).toBeInTheDocument();
  });

  it("shows the server detail text when present", async () => {
    mockCursor();
    mockAgents([
      { harness: "claude", installed: true, signedIn: false, detail: "Run `claude auth login` on the host" },
      { harness: "codex", installed: true, signedIn: true, detail: null },
      { harness: "cursor", installed: true, signedIn: true, detail: null },
      { harness: "pi", installed: true, signedIn: true, detail: null },
    ]);

    render(<AgentSignIn />);

    expect(await screen.findByText("Run `claude auth login` on the host")).toBeInTheDocument();
    expect(screen.getByTestId("agent-signin-codex")).not.toHaveTextContent("Run");
  });

  it("keeps the detail in the state cell of its own row", async () => {
    mockCursor();
    mockAgents([
      { harness: "claude", installed: true, signedIn: false, detail: "Run `claude auth login` on the host" },
      { harness: "codex", installed: true, signedIn: true, detail: null },
      { harness: "cursor", installed: true, signedIn: true, detail: null },
      { harness: "pi", installed: true, signedIn: true, detail: null },
    ]);

    render(<AgentSignIn />);

    const claudeRow = await screen.findByTestId("agent-signin-claude");
    const cell = within(claudeRow).getByRole("definition");
    expect(cell).toHaveTextContent("Signed out");
    expect(cell).toHaveTextContent("Run `claude auth login` on the host");
    expect(within(screen.getByTestId("agent-signin-codex")).getByRole("definition")).not.toHaveTextContent("Run");
  });

  it("shows the loading state before the report arrives", () => {
    mockCursor();
    vi.spyOn(api, "fetchAgentAuthStatus").mockReturnValue(new Promise(() => {}));

    render(<AgentSignIn />);

    expect(screen.getByText("Checking sign-in…")).toBeInTheDocument();
  });

  it("shows an error when the status check fails", async () => {
    mockCursor();
    vi.spyOn(api, "fetchAgentAuthStatus").mockRejectedValue(new Error("boom"));

    render(<AgentSignIn />);

    expect(await screen.findByText("Could not check sign-in.")).toBeInTheDocument();
  });

  it("renders the Cursor sign-in controls once, outside the status list", async () => {
    mockCursor();
    mockAgents([
      { harness: "claude", installed: true, signedIn: true, detail: null },
      { harness: "codex", installed: true, signedIn: true, detail: null },
      { harness: "cursor", installed: true, signedIn: false, detail: null },
      { harness: "pi", installed: true, signedIn: true, detail: null },
    ]);

    render(<AgentSignIn />);

    const button = await screen.findByTestId("cursor-sign-in");
    expect(button).toHaveTextContent("Sign in with Cursor");
    expect(screen.getAllByTestId("cursor-sign-in")).toHaveLength(1);
    expect(within(screen.getByTestId("agent-signin-list")).queryByTestId("cursor-sign-in")).not.toBeInTheDocument();
  });

  it("refetches the report after Cursor sign-in completes", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    mockCursor();
    const agentSpy = mockAgents([
      { harness: "claude", installed: true, signedIn: true, detail: null },
      { harness: "codex", installed: true, signedIn: true, detail: null },
      { harness: "cursor", installed: true, signedIn: false, detail: null },
      { harness: "pi", installed: true, signedIn: true, detail: null },
    ]);
    vi.spyOn(api, "fetchCursorLoginPhase")
      .mockResolvedValueOnce({ phase: "idle" })
      .mockResolvedValue({ phase: "succeeded", email: "op@example.com", expiresAtMs: null });
    vi.spyOn(api, "startCursorLogin").mockResolvedValue({ phase: "idle" });

    render(<AgentSignIn />);

    fireEvent.click(await screen.findByTestId("cursor-sign-in"));
    await vi.waitFor(() => expect(agentSpy).toHaveBeenCalledTimes(2), { timeout: 5000 });
  });
});
