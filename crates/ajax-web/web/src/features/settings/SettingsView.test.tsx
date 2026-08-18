import { describe, it, expect, vi, afterEach } from "vitest";
import { render, fireEvent, screen } from "@testing-library/react";
import SettingsView from "./SettingsView";
import * as api from "@/shared/lib/api";
import * as diagnostics from "./diagnostics";
import * as clipboard from "@/shared/lib/clipboard";
import * as telemetry from "@/shared/lib/telemetry";
import * as pushTest from "./pushTest";
import { TEST_IN_STABLE_TIMEOUT_MS } from "@/shared/lib/polling";

vi.mock("@/shared/lib/telemetry", async () => {
  const actual = await vi.importActual<typeof import("@/shared/lib/telemetry")>(
    "@/shared/lib/telemetry",
  );
  return {
    ...actual,
    captureTelemetryDiagnostic: vi.fn(actual.captureTelemetryDiagnostic),
    isTelemetryInitialized: vi.fn(actual.isTelemetryInitialized),
  };
});

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
    vi.spyOn(api, "waitForServerRestart").mockResolvedValue(true);
    render(<SettingsView />);
    await vi.waitFor(() =>
      expect(screen.getByRole("button", { name: "Test in Stable" })).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Test in Stable" }));
    expect(spy).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Tap to confirm" })).toBeInTheDocument();
  });

  it("starts Test in Stable and replaces location on success #850", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: true,
    });
    const startSpy = vi.spyOn(api, "startTestInStable").mockResolvedValue({
      ok: true,
      restarting: true,
    });
    const waitSpy = vi.spyOn(api, "waitForServerRestart").mockResolvedValue(true);
    const replace = vi.fn();
    const reload = vi.fn();
    vi.stubGlobal("location", {
      ...window.location,
      origin: "https://ajax.local:8787",
      hash: "#/settings",
      replace,
      reload,
    });

    render(<SettingsView />);
    await vi.waitFor(() =>
      expect(screen.getByRole("button", { name: "Test in Stable" })).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Test in Stable" }));
    fireEvent.click(screen.getByRole("button", { name: "Tap to confirm" }));
    await vi.waitFor(() => expect(startSpy).toHaveBeenCalledOnce());
    expect(waitSpy).toHaveBeenCalledWith({
      timeoutMs: TEST_IN_STABLE_TIMEOUT_MS,
      previousVersion: "1.0.0",
    });
    await vi.waitFor(() =>
      expect(replace).toHaveBeenCalledWith("https://ajax.local:8787#/settings"),
    );
    expect(reload).not.toHaveBeenCalled();

    vi.unstubAllGlobals();
  });

  it("reports a timeout when the server does not return after Test in Stable #850", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: true,
    });
    vi.spyOn(api, "startTestInStable").mockResolvedValue({
      ok: true,
      restarting: true,
    });
    vi.spyOn(api, "waitForServerRestart").mockResolvedValue(false);
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

  it("runs diagnostics only once under same-turn double click", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
    let release!: () => void;
    const pending = new Promise((resolve) => {
      release = () => resolve({ browser_mode: "Safari/browser" });
    });
    const spy = vi.spyOn(diagnostics, "buildDiagnosticsReport").mockReturnValue(pending as never);
    render(<SettingsView />);
    const button = screen.getByRole("button", { name: "Run diagnostics" });
    button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(spy).toHaveBeenCalledOnce();
    release();
    await vi.waitFor(() =>
      expect(
        screen.getByText((content) => content.includes("Safari/browser")),
      ).toBeInTheDocument(),
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

  it("toggles orchestration chat preference", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
    render(<SettingsView />);
    expect(
      screen.getByText(
        "Supported agents run with full tool access and without approval prompts.",
      ),
    ).toBeInTheDocument();
    const toggle = screen.getByTestId("orchestration-chat-toggle");
    expect(toggle).toBeChecked();
    fireEvent.click(toggle);
    expect(toggle).not.toBeChecked();
    expect(localStorage.getItem("ajax.web.session.orchestrationChat")).toBe("false");
    fireEvent.click(toggle);
    expect(toggle).toBeChecked();
    expect(localStorage.getItem("ajax.web.session.orchestrationChat")).toBe("true");
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

  it("shows telemetry status and emits diagnostic on button click", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
    vi.mocked(telemetry.isTelemetryInitialized).mockReturnValue(true);

    render(<SettingsView />);
    const telemetrySection = await vi.waitFor(() =>
      screen.getByTestId("dev-settings-telemetry"),
    );
    expect(telemetrySection).toHaveTextContent("Initialized");
    expect(telemetrySection).toHaveTextContent("yes");

    fireEvent.click(screen.getByTestId("telemetry-diagnostic"));
    expect(telemetry.captureTelemetryDiagnostic).toHaveBeenCalledOnce();
  });

  it("runs the declarative push test flow from the Actions button", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
    const runSpy = vi.spyOn(pushTest, "runPushNotificationTest").mockImplementation(
      async (onStatus) => {
        onStatus("Scheduled — close or background the app now");
        return { ok: true };
      },
    );

    render(<SettingsView />);
    fireEvent.click(screen.getByRole("button", { name: "Test push notification" }));
    await vi.waitFor(() =>
      expect(screen.getByText("Push notification scheduled.")).toBeInTheDocument(),
    );
    expect(runSpy).toHaveBeenCalledOnce();
  });

  it("enables and disables push from Settings", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
    vi.spyOn(pushTest, "enablePushNotifications").mockResolvedValue({ ok: true });
    vi.spyOn(pushTest, "disablePushNotifications").mockResolvedValue({ ok: true });
    vi.spyOn(pushTest, "getPushSubscriptionStatus")
      .mockResolvedValueOnce("disabled")
      .mockResolvedValueOnce("enabled")
      .mockResolvedValueOnce("disabled");

    render(<SettingsView />);
    await vi.waitFor(() =>
      expect(screen.getByTestId("push-subscription-status")).toHaveTextContent("Disabled"),
    );
    fireEvent.click(screen.getByRole("button", { name: "Enable push notifications" }));
    await vi.waitFor(() =>
      expect(screen.getByText("Push notifications enabled.")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("push-subscription-status")).toHaveTextContent("Enabled");
    fireEvent.click(screen.getByRole("button", { name: "Disable push notifications" }));
    await vi.waitFor(() =>
      expect(screen.getByText("Push notifications disabled.")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("push-subscription-status")).toHaveTextContent("Disabled");
  });

  it("shows unavailable push status when pushManager is missing", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
    vi.spyOn(pushTest, "getPushSubscriptionStatus").mockResolvedValue("unavailable");

    render(<SettingsView />);
    await vi.waitFor(() =>
      expect(screen.getByTestId("push-subscription-status")).toHaveTextContent("Unavailable"),
    );
  });

  it("shows push test errors from the helper", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
    vi.spyOn(pushTest, "runPushNotificationTest").mockResolvedValue({
      ok: false,
      error: "Declarative push is not supported in this browser.",
    });

    render(<SettingsView />);
    fireEvent.click(screen.getByRole("button", { name: "Test push notification" }));
    await vi.waitFor(() =>
      expect(
        screen.getByText("Declarative push is not supported in this browser."),
      ).toBeInTheDocument(),
    );
  });
});
