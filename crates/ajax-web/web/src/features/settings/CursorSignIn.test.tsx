import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, screen, act } from "@testing-library/react";
import * as api from "@/shared/lib/api";
import CursorSignIn from "./CursorSignIn";

const AUTH_URL = "https://cursor.com/sign-in?session=abc123";

function notSignedInStatus() {
  return { present: false, valid: null, email: null, expiresAtMs: null, error: null };
}

describe("CursorSignIn", () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("shows the signed-in status for a valid key", async () => {
    vi.spyOn(api, "fetchCursorAuthStatus").mockResolvedValue({
      present: true,
      valid: true,
      email: "op@example.com",
      expiresAtMs: null,
      error: null,
    });
    vi.spyOn(api, "fetchCursorLoginPhase").mockResolvedValue({ phase: "idle" });

    render(<CursorSignIn />);

    expect(await screen.findByText("Signed in as op@example.com")).toBeInTheDocument();
  });

  it("shows the rejected key message and a Sign in again button", async () => {
    vi.spyOn(api, "fetchCursorAuthStatus").mockResolvedValue({
      present: true,
      valid: false,
      email: null,
      expiresAtMs: null,
      error: "rejected",
    });
    vi.spyOn(api, "fetchCursorLoginPhase").mockResolvedValue({ phase: "idle" });

    render(<CursorSignIn />);

    await screen.findByText("Cursor rejected the saved key. Sign in again.");
    expect(screen.getByRole("button", { name: "Sign in again" })).toBeInTheDocument();
  });

  it("polls to the waiting phase and shows the sign-in link with the button disabled", async () => {
    vi.spyOn(api, "fetchCursorAuthStatus").mockResolvedValue(notSignedInStatus());
    const phaseSpy = vi
      .spyOn(api, "fetchCursorLoginPhase")
      .mockResolvedValueOnce({ phase: "idle" })
      .mockResolvedValue({ phase: "waiting", url: AUTH_URL });
    vi.spyOn(api, "startCursorLogin").mockResolvedValue({ phase: "idle" });

    render(<CursorSignIn />);

    fireEvent.click(screen.getByTestId("cursor-sign-in"));
    expect(api.startCursorLogin).toHaveBeenCalledOnce();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0); // let the start request resolve; interval is now live
    });
    vi.advanceTimersByTime(2000); // fire the first poll

    const link = await screen.findByTestId("cursor-sign-in-link");
    expect(link).toHaveAttribute("href", AUTH_URL);
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
    expect(phaseSpy).toHaveBeenCalledTimes(2); // mount resume check + one poll
    expect(screen.getByTestId("cursor-sign-in")).toBeDisabled();
  });

  it("stops polling and re-fetches status after a successful sign-in", async () => {
    const statusSpy = vi.spyOn(api, "fetchCursorAuthStatus")
      .mockResolvedValueOnce(notSignedInStatus())
      .mockResolvedValue({
        present: true,
        valid: true,
        email: "op@example.com",
        expiresAtMs: null,
        error: null,
      });
    const phaseSpy = vi
      .spyOn(api, "fetchCursorLoginPhase")
      .mockResolvedValueOnce({ phase: "idle" }) // mount resume check
      .mockResolvedValueOnce({ phase: "waiting", url: AUTH_URL })
      .mockResolvedValueOnce({ phase: "verifying" })
      .mockResolvedValueOnce({ phase: "succeeded", email: "op@example.com", expiresAtMs: null })
      .mockResolvedValue({ phase: "succeeded", email: "op@example.com", expiresAtMs: null });
    vi.spyOn(api, "startCursorLogin").mockResolvedValue({ phase: "idle" });

    render(<CursorSignIn />);

    fireEvent.click(screen.getByTestId("cursor-sign-in"));
    expect(api.startCursorLogin).toHaveBeenCalledOnce();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0); // let the start request resolve; interval is now live
    });
    // Fire the three polls one at a time so each async poll settles before
    // the next fires (avoids load-timing flake under fake timers).
    await vi.advanceTimersByTimeAsync(2000); // poll 1: waiting
    await vi.advanceTimersByTimeAsync(2000); // poll 2: verifying
    await vi.advanceTimersByTimeAsync(2000); // poll 3: succeeded
    await vi.advanceTimersByTimeAsync(0); // let post-success refresh settle

    expect(screen.getByText("Signed in as op@example.com")).toBeInTheDocument();
    expect(statusSpy).toHaveBeenCalledTimes(2); // initial load + post-success refresh
    const callsAfterSuccess = phaseSpy.mock.calls.length;
    expect(callsAfterSuccess).toBe(4); // mount + 3 polls (waiting, verifying, succeeded)

    await vi.advanceTimersByTimeAsync(10000); // polling must be stopped by now
    expect(phaseSpy).toHaveBeenCalledTimes(callsAfterSuccess);
  });

  it("cancels the sign-in and shows the failed message", async () => {
    vi.spyOn(api, "fetchCursorAuthStatus").mockResolvedValue(notSignedInStatus());
    vi.spyOn(api, "fetchCursorLoginPhase").mockResolvedValue({ phase: "waiting", url: AUTH_URL });
    const startSpy = vi.spyOn(api, "startCursorLogin").mockResolvedValue({
      phase: "waiting",
      url: AUTH_URL,
    });
    const cancelSpy = vi.spyOn(api, "cancelCursorLogin").mockResolvedValue({
      phase: "failed",
      message: "cancelled",
    });

    render(<CursorSignIn />);

    fireEvent.click(screen.getByTestId("cursor-sign-in"));
    expect(startSpy).toHaveBeenCalledOnce();
    const cancelButton = await screen.findByTestId("cursor-sign-in-cancel");
    fireEvent.click(cancelButton);

    expect(cancelSpy).toHaveBeenCalledOnce();
    expect(await screen.findByText("Sign-in failed: cancelled")).toBeInTheDocument();
  });

  it("clears polling on unmount", async () => {
    vi.spyOn(api, "fetchCursorAuthStatus").mockResolvedValue(notSignedInStatus());
    const phaseSpy = vi.spyOn(api, "fetchCursorLoginPhase").mockResolvedValue({
      phase: "waiting",
      url: AUTH_URL,
    });
    vi.spyOn(api, "startCursorLogin").mockResolvedValue({ phase: "waiting", url: AUTH_URL });

    const { unmount } = render(<CursorSignIn />);

    fireEvent.click(screen.getByTestId("cursor-sign-in"));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0); // let the start request resolve; interval is now live
    });
    vi.advanceTimersByTime(2000); // fire one poll so the interval is live
    await screen.findByTestId("cursor-sign-in-link");
    const callsBeforeUnmount = phaseSpy.mock.calls.length;

    unmount();
    await vi.advanceTimersByTimeAsync(10000);

    expect(phaseSpy).toHaveBeenCalledTimes(callsBeforeUnmount);
  });

  it("shows an error when the status check fails", async () => {
    vi.spyOn(api, "fetchCursorAuthStatus").mockRejectedValue(new Error("boom"));
    vi.spyOn(api, "fetchCursorLoginPhase").mockResolvedValue({ phase: "idle" });

    render(<CursorSignIn />);

    expect(await screen.findByText("Could not check Cursor sign-in.")).toBeInTheDocument();
  });

  it("re-enables the button and shows a failure when starting sign-in fails", async () => {
    vi.spyOn(api, "fetchCursorAuthStatus").mockResolvedValue(notSignedInStatus());
    const phaseSpy = vi.spyOn(api, "fetchCursorLoginPhase").mockResolvedValue({ phase: "idle" });
    vi.spyOn(api, "startCursorLogin").mockRejectedValue(new Error("network down"));

    render(<CursorSignIn />);

    fireEvent.click(screen.getByTestId("cursor-sign-in"));
    expect(await screen.findByText("Sign-in failed: could not start sign-in")).toBeInTheDocument();
    expect(screen.getByTestId("cursor-sign-in")).toBeEnabled();

    const callsAfterFailure = phaseSpy.mock.calls.length; // only the mount resume check
    await vi.advanceTimersByTimeAsync(6000);
    expect(phaseSpy).toHaveBeenCalledTimes(callsAfterFailure);
  });

  it("keeps the button enabled and does not poll when start returns failed", async () => {
    vi.spyOn(api, "fetchCursorAuthStatus").mockResolvedValue(notSignedInStatus());
    const phaseSpy = vi.spyOn(api, "fetchCursorLoginPhase").mockResolvedValue({ phase: "idle" });
    vi.spyOn(api, "startCursorLogin").mockResolvedValue({ phase: "failed", message: "node missing" });

    render(<CursorSignIn />);

    fireEvent.click(screen.getByTestId("cursor-sign-in"));
    expect(await screen.findByText("Sign-in failed: node missing")).toBeInTheDocument();
    expect(screen.getByTestId("cursor-sign-in")).toBeEnabled();

    const callsAfterStart = phaseSpy.mock.calls.length; // only the mount resume check
    await vi.advanceTimersByTimeAsync(6000);
    expect(phaseSpy).toHaveBeenCalledTimes(callsAfterStart);
  });
  it("refuses to render a non-https sign-in link", async () => {
    vi.spyOn(api, "fetchCursorAuthStatus").mockResolvedValue(notSignedInStatus());
    vi.spyOn(api, "fetchCursorLoginPhase").mockResolvedValue({ phase: "idle" });
    vi.spyOn(api, "startCursorLogin").mockResolvedValue({ phase: "waiting", url: "javascript:alert(1)" });

    render(<CursorSignIn />);

    fireEvent.click(screen.getByTestId("cursor-sign-in"));
    expect(await screen.findByText("Sign-in failed: unexpected sign-in link")).toBeInTheDocument();
    expect(screen.queryByTestId("cursor-sign-in-link")).not.toBeInTheDocument();
  });
});
