import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/react";
import SettingsView from "./SettingsView";
import * as api from "../api";
import * as diagnostics from "../diagnostics";

afterEach(() => {
  localStorage.clear();
  sessionStorage.clear();
  vi.restoreAllMocks();
});

describe("SettingsView", () => {
  it("requires confirmation before restarting", async () => {
    const spy = vi.spyOn(api, "restartServer").mockResolvedValue({});
    vi.spyOn(api, "waitForServerOnline").mockResolvedValue(true);
    const { getByText } = render(<SettingsView />);
    await fireEvent.click(getByText("Restart server"));
    expect(spy).not.toHaveBeenCalled();
    expect(getByText("Tap to confirm")).toBeInTheDocument();
  });

  it("restarts and reports success on the second tap", async () => {
    const spy = vi.spyOn(api, "restartServer").mockResolvedValue({});
    vi.spyOn(api, "waitForServerOnline").mockResolvedValue(true);
    const onResult = vi.fn();
    const onRestarted = vi.fn();
    const { getByText } = render(
      <SettingsView onResult={onResult} onRestarted={onRestarted} />,
    );
    await fireEvent.click(getByText("Restart server"));
    await fireEvent.click(getByText("Tap to confirm"));
    await vi.waitFor(() => expect(spy).toHaveBeenCalledOnce());
    await vi.waitFor(() =>
      expect(onResult).toHaveBeenCalledWith("Server restarted", null, false),
    );
    expect(onRestarted).toHaveBeenCalledOnce();
  });

  it("reports a timeout when the server does not return", async () => {
    vi.spyOn(api, "restartServer").mockResolvedValue({});
    vi.spyOn(api, "waitForServerOnline").mockResolvedValue(false);
    const onResult = vi.fn();
    const { getByText } = render(<SettingsView onResult={onResult} />);
    await fireEvent.click(getByText("Restart server"));
    await fireEvent.click(getByText("Tap to confirm"));
    await vi.waitFor(() =>
      expect(onResult).toHaveBeenCalledWith("Server did not come back in time", null, true),
    );
  });

  it("renders the diagnostics report", async () => {
    vi.spyOn(diagnostics, "buildDiagnosticsReport").mockResolvedValue({
      browser_mode: "Safari/browser",
    });
    const { getByText, container } = render(<SettingsView />);
    await fireEvent.click(getByText("Run diagnostics"));
    await vi.waitFor(() =>
      expect(container.querySelector(".settings-status")?.textContent).toContain("Safari/browser"),
    );
  });

  it("copies diagnostics with a clipboard fallback message", async () => {
    vi.spyOn(diagnostics, "buildDiagnosticsReport").mockResolvedValue({ ok: true });
    vi.spyOn(diagnostics, "copyText").mockResolvedValue(false);
    const onResult = vi.fn();
    const { getByText } = render(<SettingsView onResult={onResult} />);
    await fireEvent.click(getByText("Copy Diagnostics"));
    await vi.waitFor(() =>
      expect(onResult).toHaveBeenCalledWith("Diagnostics ready to copy", null, false),
    );
  });

  it("renders Diagnostics debug info", () => {
    const { getByText, getByTestId } = render(<SettingsView />);
    expect(getByTestId("dev-settings")).toBeInTheDocument();
    expect(getByText("Diagnostics")).toBeInTheDocument();
  });

  it("shows live debug info with origin and app version", () => {
    const meta = document.createElement("meta");
    meta.name = "ajax-app-version";
    meta.content = "0.42.0-test";
    document.head.appendChild(meta);

    const { getByTestId } = render(<SettingsView />);
    const debug = getByTestId("dev-settings-debug");
    expect(debug.textContent).toContain(window.location.origin);
    expect(debug.textContent).toContain("0.42.0-test");

    meta.remove();
  });

  it("reload app restarts the server then reloads the page", async () => {
    const restartSpy = vi.spyOn(api, "restartServer").mockResolvedValue({});
    vi.spyOn(api, "waitForServerOnline").mockResolvedValue(true);
    const reload = vi.fn();
    vi.stubGlobal("location", { ...window.location, reload });

    const { getByText } = render(<SettingsView />);
    await fireEvent.click(getByText("Reload app"));
    await vi.waitFor(() => expect(restartSpy).toHaveBeenCalledOnce());
    expect(reload).toHaveBeenCalledOnce();

    vi.unstubAllGlobals();
  });

  it("reload app reports timeout when the server does not return", async () => {
    vi.spyOn(api, "restartServer").mockResolvedValue({});
    vi.spyOn(api, "waitForServerOnline").mockResolvedValue(false);
    const reload = vi.fn();
    vi.stubGlobal("location", { ...window.location, reload });
    const onResult = vi.fn();

    const { getByText } = render(<SettingsView onResult={onResult} />);
    await fireEvent.click(getByText("Reload app"));
    await vi.waitFor(() =>
      expect(onResult).toHaveBeenCalledWith("Server did not come back in time", null, true),
    );
    expect(reload).not.toHaveBeenCalled();

    vi.unstubAllGlobals();
  });
});
