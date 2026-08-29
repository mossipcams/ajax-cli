import { useEffect, useRef, useState } from "react";
import {
  startTestInStable,
  TEST_IN_STABLE_TIMEOUT_MS,
  waitForServerRestart,
} from "@/shared/lib/api";
import { useFetchVersion, useVersionQuery } from "@/shared/hooks/useVersionQuery";
import { buildDiagnosticsReport } from "./diagnostics";
import { copyText } from "@/shared/lib/clipboard";
import { CONFIRM_TIMEOUT_MS } from "@/shared/lib/polling";
import {
  captureTelemetryDiagnostic,
  isStandaloneDisplay,
  isTelemetryInitialized,
  readAppVersion,
} from "@/shared/lib/telemetry";
import {
  readOrchestrationChatEnabled,
  writeOrchestrationChatEnabled,
} from "./orchestrationChatPreference";
import { Button } from "@/shared/ui/button";
import {
  runPushNotificationTest,
  enablePushNotifications,
  disablePushNotifications,
  getPushSubscriptionStatus,
  type PushSubscriptionStatus,
} from "./pushTest";

interface Props {
  detailHandle?: string | null;
  onResult?: (message: string, output: string | null | undefined, isError: boolean) => void;
  onRestarted?: () => void;
  onBack?: () => void;
}

export default function SettingsView({
  detailHandle = null,
  onResult,
  onBack,
}: Props) {
  const [testInStableAvailable, setTestInStableAvailable] = useState(false);
  const [confirmingTestInStable, setConfirmingTestInStable] = useState(false);
  const [testInStableStatus, setTestInStableStatus] = useState<string | null>(null);
  const [testingInStable, setTestingInStable] = useState(false);
  const [diagnosticsOutput, setDiagnosticsOutput] = useState<string | null>(null);
  const [pushTestStatus, setPushTestStatus] = useState<string | null>(null);
  const [pushSubscriptionStatus, setPushSubscriptionStatus] =
    useState<PushSubscriptionStatus>("disabled");
  const [testingPush, setTestingPush] = useState(false);
  const [orchestrationChat, setOrchestrationChat] = useState(readOrchestrationChatEnabled);
  const confirmTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const diagnosticsLatchRef = useRef(false);
  const { data: versionInfo } = useVersionQuery();
  const fetchVersion = useFetchVersion();

  async function refreshPushSubscriptionStatus() {
    setPushSubscriptionStatus(await getPushSubscriptionStatus());
  }

  useEffect(() => {
    let cancelled = false;
    void getPushSubscriptionStatus().then((status) => {
      if (!cancelled) {
        setPushSubscriptionStatus(status);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    setTestInStableAvailable(versionInfo?.test_in_stable === true);
  }, [versionInfo]);

  const testInStableRestartsThisInstance = versionInfo?.profile === "stable";

  async function testInStable() {
    if (!confirmingTestInStable) {
      setConfirmingTestInStable(true);
      confirmTimer.current = setTimeout(
        () => setConfirmingTestInStable(false),
        CONFIRM_TIMEOUT_MS,
      );
      return;
    }
    if (confirmTimer.current) clearTimeout(confirmTimer.current);
    setConfirmingTestInStable(false);
    setTestingInStable(true);
    setTestInStableStatus(
      testInStableRestartsThisInstance
        ? "Pulling main and rebuilding…"
        : "Pulling main and rebuilding stable Cockpit…",
    );
    try {
      const response = await startTestInStable();
      if (!response.restarting) {
        setTestingInStable(false);
        setTestInStableStatus(null);
        onResult?.("Test in Stable started on stable Cockpit", null, false);
        return;
      }
    } catch {
      if (!testInStableRestartsThisInstance) {
        setTestingInStable(false);
        setTestInStableStatus(null);
        onResult?.("Test in Stable failed", null, true);
        return;
      }
      // Stable exits mid-request; connection drop is expected (#850).
    }
    const resumeOrigin = window.location.origin;
    const resumeHash = window.location.hash;
    const resumeOriginOk =
      resumeOrigin.startsWith("http://") || resumeOrigin.startsWith("https://");
    let previousVersion: string | null = null;
    try {
      const version = await fetchVersion();
      previousVersion = version.version;
    } catch {
      // Continue without a version baseline.
    }
    const online = await waitForServerRestart({
      timeoutMs: TEST_IN_STABLE_TIMEOUT_MS,
      previousVersion,
    });
    setTestingInStable(false);
    if (online && resumeOriginOk) {
      setTestInStableStatus(null);
      window.location.replace(`${resumeOrigin}${resumeHash}`);
      return;
    }
    setTestInStableStatus(null);
    onResult?.("Server did not come back in time", null, true);
  }

  async function runDiagnostics() {
    if (diagnosticsLatchRef.current) return;
    diagnosticsLatchRef.current = true;
    try {
      setDiagnosticsOutput("Running diagnostics...");
      const report = await buildDiagnosticsReport(detailHandle);
      setDiagnosticsOutput(JSON.stringify(report, null, 2));
    } finally {
      diagnosticsLatchRef.current = false;
    }
  }

  async function copyDiagnostics() {
    if (diagnosticsLatchRef.current) return;
    diagnosticsLatchRef.current = true;
    try {
      const report = await buildDiagnosticsReport(detailHandle);
      const text = JSON.stringify(report, null, 2);
      setDiagnosticsOutput(text);
      const copied = await copyText(text);
      onResult?.(copied ? "Diagnostics copied" : "Diagnostics ready to copy", null, false);
    } finally {
      diagnosticsLatchRef.current = false;
    }
  }

  async function enablePush() {
    setTestingPush(true);
    setPushTestStatus(null);
    const result = await enablePushNotifications(setPushTestStatus);
    setTestingPush(false);
    setPushTestStatus(result.ok ? "Push notifications enabled." : result.error);
    if (result.ok) {
      await refreshPushSubscriptionStatus();
    }
  }

  async function disablePush() {
    setTestingPush(true);
    setPushTestStatus(null);
    const result = await disablePushNotifications(setPushTestStatus);
    setTestingPush(false);
    setPushTestStatus(result.ok ? "Push notifications disabled." : result.error);
    if (result.ok) {
      await refreshPushSubscriptionStatus();
    }
  }

  async function testPushNotification() {
    setTestingPush(true);
    setPushTestStatus(null);
    const result = await runPushNotificationTest(setPushTestStatus);
    setTestingPush(false);
    if (result.ok) {
      setPushTestStatus("Push notification scheduled.");
      return;
    }
    setPushTestStatus(result.error);
  }

  const appVersion =
    readAppVersion() ??
    document.querySelector<HTMLMetaElement>('meta[name="ajax-app-version"]')?.content ??
    "—";
  const telemetryInitialized = isTelemetryInitialized();
  const telemetryStandalone = isStandaloneDisplay();
  const origin = window.location.origin;
  const online = navigator.onLine;
  const truncatedUa =
    navigator.userAgent.length > 80
      ? `${navigator.userAgent.slice(0, 80)}…`
      : navigator.userAgent;

  return (
    <section className="settings-view" aria-labelledby="settings-heading">
      <div className="settings-header">
        <Button type="button" variant="secondary" className="settings-back" onClick={() => onBack?.()}>
          Back
        </Button>
        <h2 id="settings-heading">Settings</h2>
      </div>

      <div className="settings-section" data-testid="ajax-web-session-settings">
        <h3>Ajax web session</h3>
        <p className="settings-note">
          Chat-first orchestration session with inline artifacts. Use Ajax terminal in task details
          to switch a task to the embedded terminal view.
        </p>
        <label className="settings-toggle" htmlFor="orchestration-chat-toggle">
          <input
            id="orchestration-chat-toggle"
            data-testid="orchestration-chat-toggle"
            type="checkbox"
            aria-describedby="orchestration-chat-description"
            checked={orchestrationChat}
            onChange={(event) => {
              const enabled = event.target.checked;
              setOrchestrationChat(enabled);
              writeOrchestrationChatEnabled(enabled);
            }}
          />
          Orchestration chat session
        </label>
        <p id="orchestration-chat-description" className="settings-note">
          Supported agents run with full tool access and without approval prompts.
        </p>
      </div>

      <div className="settings-section" data-testid="dev-settings">
        <h3>Diagnostics</h3>

        <h4 className="settings-subheading">Telemetry</h4>
        <dl className="settings-debug" data-testid="dev-settings-telemetry">
          <div>
            <dt>Initialized</dt>
            <dd>{telemetryInitialized ? "yes" : "no"}</dd>
          </div>
          <div>
            <dt>Standalone</dt>
            <dd>{telemetryStandalone ? "yes" : "no"}</dd>
          </div>
          <div>
            <dt>App version</dt>
            <dd>{appVersion}</dd>
          </div>
        </dl>
        <Button
          type="button"
          variant="secondary"
          data-testid="telemetry-diagnostic"
          onClick={() => captureTelemetryDiagnostic()}
        >
          Emit telemetry diagnostic
        </Button>

        <h4 className="settings-subheading">Debug info</h4>
        <dl className="settings-debug" data-testid="dev-settings-debug">
          <div>
            <dt>App version</dt>
            <dd>{appVersion}</dd>
          </div>
          <div>
            <dt>Origin</dt>
            <dd>{origin}</dd>
          </div>
          <div>
            <dt>Online</dt>
            <dd>{online ? "yes" : "no"}</dd>
          </div>
          <div>
            <dt>User agent</dt>
            <dd>{truncatedUa}</dd>
          </div>
        </dl>

        <h4 className="settings-subheading">Push notifications</h4>
        <dl className="settings-debug" data-testid="dev-settings-push">
          <div>
            <dt>Status</dt>
            <dd data-testid="push-subscription-status">
              {pushSubscriptionStatus === "enabled"
                ? "Enabled"
                : pushSubscriptionStatus === "disabled"
                  ? "Disabled"
                  : "Unavailable"}
            </dd>
          </div>
        </dl>

        <h4 className="settings-subheading">Actions</h4>
        <Button type="button" variant="secondary" onClick={runDiagnostics}>
          Run diagnostics
        </Button>
        <Button type="button" variant="secondary" onClick={copyDiagnostics}>
          Copy Diagnostics
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={testingPush}
          onClick={enablePush}
        >
          Enable push notifications
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={testingPush}
          onClick={disablePush}
        >
          Disable push notifications
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={testingPush}
          onClick={testPushNotification}
        >
          Test push notification
        </Button>
        {pushTestStatus ? <p className="settings-status">{pushTestStatus}</p> : null}
        {testInStableAvailable ? (
          <>
            <p className="settings-note">
              {testInStableRestartsThisInstance
                ? "Pulls origin/main, rebuilds, and restarts this stable Cockpit."
                : "Pulls origin/main, rebuilds, and restarts stable Cockpit on port 8787."}
            </p>
            <Button
              type="button"
              variant="secondary"
              disabled={testingInStable}
              onClick={testInStable}
            >
              {confirmingTestInStable ? "Tap to confirm" : "Test in Stable"}
            </Button>
          </>
        ) : null}
        {testInStableStatus ? <p className="settings-status">{testInStableStatus}</p> : null}
        {diagnosticsOutput ? <pre className="settings-status">{diagnosticsOutput}</pre> : null}
      </div>
    </section>
  );
}
