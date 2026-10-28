import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, screen, within } from "@testing-library/react";
import SettingsView from "./SettingsView";
import * as api from "@/shared/lib/api";
import * as diagnostics from "./diagnostics";
import * as clipboard from "@/shared/lib/clipboard";
import * as pushTest from "./pushTest";
import { TEST_IN_STABLE_TIMEOUT_MS } from "@/shared/lib/polling";

const okCheck: diagnostics.DiagnosticCheck = { ok: true, status: 200, error: null, body: null };

function diagnosticsReport(
  checks: Record<string, diagnostics.DiagnosticCheck> = { health: okCheck },
): diagnostics.DiagnosticsReport {
  return {
    browser_mode: "Safari/browser",
    backend_url: "http://localhost:3000",
    navigator_onLine: true,
    app_version: null,
    location: "http://localhost:3000/",
    checks,
  };
}

beforeEach(() => {
  vi.spyOn(api, "fetchAgentAuthStatus").mockResolvedValue({ harnesses: [] });
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
      profile: "stable",
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

  it("waits and replaces when Test in Stable POST fails on stable profile #850", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: true,
      profile: "stable",
    });
    const startSpy = vi.spyOn(api, "startTestInStable").mockRejectedValue(new Error("Failed to fetch"));
    const waitSpy = vi.spyOn(api, "waitForServerRestart").mockResolvedValue(true);
    const replace = vi.fn();
    vi.stubGlobal("location", {
      ...window.location,
      origin: "https://ajax.local:8787",
      hash: "#/settings",
      replace,
    });
    const onResult = vi.fn();

    render(<SettingsView onResult={onResult} />);
    await vi.waitFor(() =>
      expect(screen.getByRole("button", { name: "Test in Stable" })).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Test in Stable" }));
    fireEvent.click(screen.getByRole("button", { name: "Tap to confirm" }));
    await vi.waitFor(() => expect(startSpy).toHaveBeenCalledOnce());
    await vi.waitFor(() =>
      expect(waitSpy).toHaveBeenCalledWith({
        timeoutMs: TEST_IN_STABLE_TIMEOUT_MS,
        previousVersion: "1.0.0",
      }),
    );
    await vi.waitFor(() =>
      expect(replace).toHaveBeenCalledWith("https://ajax.local:8787#/settings"),
    );
    expect(onResult).not.toHaveBeenCalledWith("Test in Stable failed", null, true);

    vi.unstubAllGlobals();
  });

  it("starts Test in Stable and replaces location on success #850", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: true,
      profile: "stable",
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
    await vi.waitFor(() =>
      expect(waitSpy).toHaveBeenCalledWith({
        timeoutMs: TEST_IN_STABLE_TIMEOUT_MS,
        previousVersion: "1.0.0",
      }),
    );
    await vi.waitFor(() =>
      expect(replace).toHaveBeenCalledWith("https://ajax.local:8787#/settings"),
    );
    expect(reload).not.toHaveBeenCalled();

    vi.unstubAllGlobals();
  });

  it("starts Test in Stable on dev without waiting for restart", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: true,
      profile: "dev",
    });
    const startSpy = vi.spyOn(api, "startTestInStable").mockResolvedValue({
      ok: true,
      restarting: false,
    });
    const waitSpy = vi.spyOn(api, "waitForServerRestart");
    const replace = vi.fn();
    vi.stubGlobal("location", {
      ...window.location,
      replace,
    });
    const onResult = vi.fn();

    render(<SettingsView onResult={onResult} />);
    await vi.waitFor(() =>
      expect(screen.getByRole("button", { name: "Test in Stable" })).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Test in Stable" }));
    fireEvent.click(screen.getByRole("button", { name: "Tap to confirm" }));
    await vi.waitFor(() => expect(startSpy).toHaveBeenCalledOnce());
    expect(waitSpy).not.toHaveBeenCalled();
    expect(replace).not.toHaveBeenCalled();
    expect(onResult).toHaveBeenCalledWith(
      "Test in Stable started on stable Cockpit",
      null,
      false,
    );

    vi.unstubAllGlobals();
  });

  it("reports a timeout when the server does not return after Test in Stable #850", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: true,
      profile: "stable",
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

  it("copies diagnostics only once under same-turn double click", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
    let release!: () => void;
    const pending = new Promise((resolve) => {
      release = () => resolve(diagnosticsReport());
    });
    const spy = vi.spyOn(diagnostics, "buildDiagnosticsReport").mockReturnValue(pending as never);
    vi.spyOn(clipboard, "copyText").mockResolvedValue(true);
    render(<SettingsView />);
    const button = screen.getByRole("button", { name: "Copy diagnostics report" });
    button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(spy).toHaveBeenCalledOnce();
    release();
    await vi.waitFor(() => expect(screen.getByTestId("diagnostics-checks")).toBeInTheDocument());
  });

  it("copies diagnostics with a clipboard fallback message", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
    vi.spyOn(diagnostics, "buildDiagnosticsReport").mockResolvedValue(diagnosticsReport());
    vi.spyOn(clipboard, "copyText").mockResolvedValue(false);
    const onResult = vi.fn();
    render(<SettingsView onResult={onResult} />);
    fireEvent.click(screen.getByRole("button", { name: "Copy diagnostics report" }));
    await vi.waitFor(() =>
      expect(onResult).toHaveBeenCalledWith("Diagnostics ready to copy", null, false),
    );
  });

  it("renders a per-check summary with ok and failure rows after copying", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
    vi.spyOn(diagnostics, "buildDiagnosticsReport").mockResolvedValue(
      diagnosticsReport({
        health: { ok: true, status: 200, error: null, body: null },
        version: { ok: true, status: 200, error: null, body: null },
        cockpit: { ok: false, status: 503, error: null, body: null },
        task: { ok: false, status: null, error: "Failed to fetch", body: null },
      }),
    );
    vi.spyOn(clipboard, "copyText").mockResolvedValue(true);
    render(<SettingsView />);
    expect(screen.queryByTestId("diagnostics-checks")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Copy diagnostics report" }));
    const summary = await vi.waitFor(() => screen.getByTestId("diagnostics-checks"));
    expect(within(summary).getAllByText("ok")).toHaveLength(2);
    expect(within(summary).getByText("cockpit")).toBeInTheDocument();
    expect(within(summary).getByText("HTTP 503")).toBeInTheDocument();
    expect(within(summary).getByText("task")).toBeInTheDocument();
    expect(within(summary).getByText("Failed to fetch")).toBeInTheDocument();
    expect(summary).not.toHaveTextContent("browser_mode");
  });

  it("keeps the full report JSON in a collapsed details element after copying", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
    vi.spyOn(diagnostics, "buildDiagnosticsReport").mockResolvedValue(diagnosticsReport());
    vi.spyOn(clipboard, "copyText").mockResolvedValue(false);
    render(<SettingsView />);
    expect(screen.queryByTestId("diagnostics-report")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Copy diagnostics report" }));
    const details = await vi.waitFor(() => screen.getByTestId("diagnostics-report"));
    expect(details.tagName).toBe("DETAILS");
    expect(details).not.toHaveAttribute("open");
    expect(within(details).getByText("Show report")).toBeInTheDocument();
    expect(details).toHaveTextContent('"checks"');
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

  it("shows the app version once in the Diagnostics section", async () => {
    vi.spyOn(api, "fetchVersion").mockResolvedValue({
      version: "1.0.0",
      test_in_stable: false,
    });
    const meta = document.createElement("meta");
    meta.name = "ajax-app-version";
    meta.content = "0.42.0-test";
    document.head.appendChild(meta);

    render(<SettingsView />);
    const diagnosticsSection = screen.getByTestId("dev-settings");
    expect(within(diagnosticsSection).getByText("App version 0.42.0-test")).toBeInTheDocument();
    expect(screen.getAllByText(/App version/)).toHaveLength(1);

    meta.remove();
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
