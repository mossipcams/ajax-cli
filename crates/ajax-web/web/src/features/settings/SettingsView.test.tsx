import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent, screen } from "@testing-library/react";
import SettingsView from "./SettingsView";
import * as api from "@/shared/lib/api";
import * as diagnostics from "./diagnostics";
import * as clipboard from "@/shared/lib/clipboard";

afterEach(() => {
  localStorage.clear();
  sessionStorage.clear();
  vi.restoreAllMocks();
});

describe("SettingsView", () => {
  it("requires confirmation before restarting", () => {
    const spy = vi.spyOn(api, "restartServer").mockResolvedValue({});
    vi.spyOn(api, "waitForServerOnline").mockResolvedValue(true);
    render(<SettingsView />);
    fireEvent.click(screen.getByRole("button", { name: "Restart server" }));
    expect(spy).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Tap to confirm" })).toBeInTheDocument();
  });

  it("restarts and reports success on the second tap", async () => {
    const spy = vi.spyOn(api, "restartServer").mockResolvedValue({});
    vi.spyOn(api, "waitForServerOnline").mockResolvedValue(true);
    const onResult = vi.fn();
    const onRestarted = vi.fn();
    render(<SettingsView onResult={onResult} onRestarted={onRestarted} />);
    fireEvent.click(screen.getByRole("button", { name: "Restart server" }));
    fireEvent.click(screen.getByRole("button", { name: "Tap to confirm" }));
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
    render(<SettingsView onResult={onResult} />);
    fireEvent.click(screen.getByRole("button", { name: "Restart server" }));
    fireEvent.click(screen.getByRole("button", { name: "Tap to confirm" }));
    await vi.waitFor(() =>
      expect(onResult).toHaveBeenCalledWith("Server did not come back in time", null, true),
    );
  });

  it("renders the diagnostics report", async () => {
    vi.spyOn(diagnostics, "buildDiagnosticsReport").mockResolvedValue({
      browser_mode: "Safari/browser",
    });
    render(<SettingsView />);
    fireEvent.click(screen.getByRole("button", { name: "Run diagnostics" }));
    await vi.waitFor(() =>
      expect(
        screen.getByText((content) => content.includes("Safari/browser")),
      ).toHaveClass("settings-status"),
    );
  });

  it("copies diagnostics with a clipboard fallback message", async () => {
    vi.spyOn(diagnostics, "buildDiagnosticsReport").mockResolvedValue({ ok: true });
    vi.spyOn(clipboard, "copyText").mockResolvedValue(false);
    const onResult = vi.fn();
    render(<SettingsView onResult={onResult} />);
    fireEvent.click(screen.getByRole("button", { name: "Copy Diagnostics" }));
    await vi.waitFor(() =>
      expect(onResult).toHaveBeenCalledWith("Diagnostics ready to copy", null, false),
    );
  });

  it("renders Diagnostics debug info", () => {
    render(<SettingsView />);
    expect(screen.getByTestId("dev-settings")).toBeInTheDocument();
    expect(screen.getByText("Diagnostics")).toBeInTheDocument();
  });

  it("shows live debug info with origin and app version", () => {
    const meta = document.createElement("meta");
    meta.name = "ajax-app-version";
    meta.content = "0.42.0-test";
    document.head.appendChild(meta);

    render(<SettingsView />);
    const debug = screen.getByTestId("dev-settings-debug");
    expect(debug).toHaveTextContent(window.location.origin);
    expect(debug).toHaveTextContent("0.42.0-test");

    meta.remove();
  });

  it("reload app restarts the server then reloads the page", async () => {
    const restartSpy = vi.spyOn(api, "restartServer").mockResolvedValue({});
    vi.spyOn(api, "waitForServerOnline").mockResolvedValue(true);
    const reload = vi.fn();
    vi.stubGlobal("location", { ...window.location, reload });

    render(<SettingsView />);
    fireEvent.click(screen.getByRole("button", { name: "Reload app" }));
    await vi.waitFor(() => expect(restartSpy).toHaveBeenCalledOnce());
    await vi.waitFor(() => expect(reload).toHaveBeenCalledOnce());

    vi.unstubAllGlobals();
  });

  it("reload app reports timeout when the server does not return", async () => {
    vi.spyOn(api, "restartServer").mockResolvedValue({});
    vi.spyOn(api, "waitForServerOnline").mockResolvedValue(false);
    const reload = vi.fn();
    vi.stubGlobal("location", { ...window.location, reload });
    const onResult = vi.fn();

    render(<SettingsView onResult={onResult} />);
    fireEvent.click(screen.getByRole("button", { name: "Reload app" }));
    await vi.waitFor(() =>
      expect(onResult).toHaveBeenCalledWith("Server did not come back in time", null, true),
    );
    expect(reload).not.toHaveBeenCalled();

    vi.unstubAllGlobals();
  });
});