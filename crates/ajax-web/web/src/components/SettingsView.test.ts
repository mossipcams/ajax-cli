import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import SettingsView from "./SettingsView.svelte";
import * as api from "../api";
import * as diagnostics from "../diagnostics";

afterEach(() => vi.restoreAllMocks());

describe("SettingsView", () => {
  it("requires confirmation before restarting", async () => {
    const spy = vi.spyOn(api, "restartServer").mockResolvedValue({});
    vi.spyOn(api, "waitForServerOnline").mockResolvedValue(true);
    const { getByText } = render(SettingsView);
    await fireEvent.click(getByText("Restart server"));
    expect(spy).not.toHaveBeenCalled();
    expect(getByText("Tap to confirm")).toBeInTheDocument();
  });

  it("restarts and reports success on the second tap", async () => {
    const spy = vi.spyOn(api, "restartServer").mockResolvedValue({});
    vi.spyOn(api, "waitForServerOnline").mockResolvedValue(true);
    const onResult = vi.fn();
    const onRestarted = vi.fn();
    const { getByText } = render(SettingsView, { props: { onResult, onRestarted } });
    await fireEvent.click(getByText("Restart server"));
    await fireEvent.click(getByText("Tap to confirm"));
    expect(spy).toHaveBeenCalledOnce();
    expect(onResult).toHaveBeenCalledWith("Server restarted", null, false);
    expect(onRestarted).toHaveBeenCalledOnce();
  });

  it("resubmits restart with the server confirmation token", async () => {
    const spy = vi
      .spyOn(api, "restartServer")
      .mockResolvedValueOnce({ ok: false, confirmation_token: "restart-token" })
      .mockResolvedValueOnce({ ok: true, restarting: true });
    vi.spyOn(api, "waitForServerOnline").mockResolvedValue(true);
    const { getByText } = render(SettingsView);

    await fireEvent.click(getByText("Restart server"));
    await fireEvent.click(getByText("Tap to confirm"));

    expect(spy).toHaveBeenCalledTimes(2);
    expect(spy).toHaveBeenNthCalledWith(1);
    expect(spy).toHaveBeenNthCalledWith(2, "restart-token");
  });

  it("reports a timeout when the server does not return", async () => {
    vi.spyOn(api, "restartServer").mockResolvedValue({});
    vi.spyOn(api, "waitForServerOnline").mockResolvedValue(false);
    const onResult = vi.fn();
    const { getByText } = render(SettingsView, { props: { onResult } });
    await fireEvent.click(getByText("Restart server"));
    await fireEvent.click(getByText("Tap to confirm"));
    expect(onResult).toHaveBeenCalledWith("Server did not come back in time", null, true);
  });

  it("renders the diagnostics report", async () => {
    vi.spyOn(diagnostics, "buildDiagnosticsReport").mockResolvedValue({ browser_mode: "Safari/browser" });
    const { getByText, container } = render(SettingsView);
    await fireEvent.click(getByText("Run diagnostics"));
    await vi.waitFor(() =>
      expect(container.querySelector(".settings-status")?.textContent).toContain("Safari/browser"),
    );
  });

  it("copies diagnostics with a clipboard fallback message", async () => {
    vi.spyOn(diagnostics, "buildDiagnosticsReport").mockResolvedValue({ ok: true });
    vi.spyOn(diagnostics, "copyText").mockResolvedValue(false);
    const onResult = vi.fn();
    const { getByText } = render(SettingsView, { props: { onResult } });
    await fireEvent.click(getByText("Copy Diagnostics"));
    await vi.waitFor(() =>
      expect(onResult).toHaveBeenCalledWith("Diagnostics ready to copy", null, false),
    );
  });
});
