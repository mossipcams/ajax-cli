import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent, screen } from "@testing-library/react";
import SettingsView from "./SettingsView";
import * as api from "@/shared/lib/api";
import * as diagnostics from "./diagnostics";
import * as clipboard from "@/shared/lib/clipboard";
import { TEST_IN_STABLE_TIMEOUT_MS } from "@/shared/lib/polling";

afterEach(() => {
  localStorage.clear();
  sessionStorage.clear();
  vi.restoreAllMocks();
});

describe("SettingsView", () => {
  it("hides Test in Stable when fetchVersion returns test_in_stable false", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
    render(<SettingsView />);
    await vi.waitFor(() => expect(api.fetchVersion).toHaveBeenCalledOnce());
    expect(screen.queryByRole("button", { name: "Test in Stable" })).not.toBeInTheDocument();
  });

  it("requires confirmation before Test in Stable", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: true,
    });
    const spy = vi.spyOn(api, "startTestInStable").mockResolvedValue({
      ok: true,
      restarting: true,
    });
    vi.spyOn(api, "waitForServerOnline").mockResolvedValue(true);
    render(<SettingsView />);
    await vi.waitFor(() =>
      expect(screen.getByRole("button", { name: "Test in Stable" })).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Test in Stable" }));
    expect(spy).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Tap to confirm" })).toBeInTheDocument();
  });

  it("starts Test in Stable and reloads the page on success", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: true,
    });
    const startSpy = vi.spyOn(api, "startTestInStable").mockResolvedValue({
      ok: true,
      restarting: true,
    });
    const waitSpy = vi.spyOn(api, "waitForServerOnline").mockResolvedValue(true);
    const reload = vi.fn();
    vi.stubGlobal("location", { ...window.location, reload });

    render(<SettingsView />);
    await vi.waitFor(() =>
      expect(screen.getByRole("button", { name: "Test in Stable" })).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Test in Stable" }));
    fireEvent.click(screen.getByRole("button", { name: "Tap to confirm" }));
    await vi.waitFor(() => expect(startSpy).toHaveBeenCalledOnce());
    expect(waitSpy).toHaveBeenCalledWith(TEST_IN_STABLE_TIMEOUT_MS);
    await vi.waitFor(() => expect(reload).toHaveBeenCalledOnce());

    vi.unstubAllGlobals();
  });

  it("reports a timeout when the server does not return after Test in Stable", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: true,
    });
    vi.spyOn(api, "startTestInStable").mockResolvedValue({
      ok: true,
      restarting: true,
    });
    vi.spyOn(api, "waitForServerOnline").mockResolvedValue(false);
    const onResult = vi.fn();
    render(<SettingsView onResult={onResult} />);
    await vi.waitFor(() =>
      expect(screen.getByRole("button", { name: "Test in Stable" })).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Test in Stable" }));
    fireEvent.click(screen.getByRole("button", { name: "Tap to confirm" }));
    await vi.waitFor(() =>
      expect(onResult).toHaveBeenCalledWith("Server did not come back in time", null, true),
    );
  });

  it("renders the diagnostics report", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
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
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
    vi.spyOn(diagnostics, "buildDiagnosticsReport").mockResolvedValue({ ok: true });
    vi.spyOn(clipboard, "copyText").mockResolvedValue(false);
    const onResult = vi.fn();
    render(<SettingsView onResult={onResult} />);
    fireEvent.click(screen.getByRole("button", { name: "Copy Diagnostics" }));
    await vi.waitFor(() =>
      expect(onResult).toHaveBeenCalledWith("Diagnostics ready to copy", null, false),
    );
  });

  it("renders Diagnostics debug info", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
    render(<SettingsView />);
    expect(screen.getByTestId("dev-settings")).toBeInTheDocument();
    expect(screen.getByText("Diagnostics")).toBeInTheDocument();
  });

  it("shows live debug info with origin and app version", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
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
});
