import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import SettingsView from "./SettingsView.svelte";
import * as api from "../api";
import * as diagnostics from "../diagnostics";
import * as setting from "../terminalSurfaceSetting";

afterEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

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

  it("renders Experimental / Terminal Surface V2", () => {
    const { getByText, getByTestId } = render(SettingsView);
    expect(getByText("Experimental")).toBeInTheDocument();
    expect(getByText("Terminal Surface V2")).toBeInTheDocument();
    expect(getByTestId("setting-terminal-surface-v2")).toBeInTheDocument();
  });

  it("toggle calls setter and reflects storage", async () => {
    const setter = vi.spyOn(setting, "setTerminalSurfaceV2Enabled");
    const { getByTestId } = render(SettingsView);
    const toggle = getByTestId("setting-terminal-surface-v2") as HTMLInputElement;
    expect(toggle.checked).toBe(false);
    await fireEvent.click(toggle);
    expect(setter).toHaveBeenCalledWith(true);
    expect(localStorage.getItem("ajax.terminal.surfaceV2")).toBe("true");
    expect(toggle.checked).toBe(true);
  });
});
