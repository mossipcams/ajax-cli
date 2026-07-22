import { useEffect, useRef, useState } from "react";
import {
  fetchVersion,
  startTestInStable,
  TEST_IN_STABLE_TIMEOUT_MS,
  waitForServerOnline,
} from "@/shared/lib/api";
import { buildDiagnosticsReport } from "./diagnostics";
import { copyText } from "@/shared/lib/clipboard";
import { CONFIRM_TIMEOUT_MS } from "@/shared/lib/polling";
import { Button } from "@/shared/ui/button";

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
  const confirmTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let cancelled = false;
    void fetchVersion()
      .then((version) => {
        if (!cancelled) {
          setTestInStableAvailable(version.test_in_stable === true);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setTestInStableAvailable(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

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
    setTestInStableStatus("Pulling main and rebuilding…");
    try {
      await startTestInStable();
    } catch {
      // A connection drop during restart is expected.
    }
    const online = await waitForServerOnline(TEST_IN_STABLE_TIMEOUT_MS);
    setTestingInStable(false);
    if (online) {
      setTestInStableStatus(null);
      window.location.reload();
      return;
    }
    setTestInStableStatus(null);
    onResult?.("Server did not come back in time", null, true);
  }

  async function runDiagnostics() {
    setDiagnosticsOutput("Running diagnostics...");
    const report = await buildDiagnosticsReport(detailHandle);
    setDiagnosticsOutput(JSON.stringify(report, null, 2));
  }

  async function copyDiagnostics() {
    const report = await buildDiagnosticsReport(detailHandle);
    const text = JSON.stringify(report, null, 2);
    setDiagnosticsOutput(text);
    const copied = await copyText(text);
    onResult?.(copied ? "Diagnostics copied" : "Diagnostics ready to copy", null, false);
  }

  const appVersion =
    document.querySelector<HTMLMetaElement>('meta[name="ajax-app-version"]')?.content ?? "—";
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

      <div className="settings-section" data-testid="dev-settings">
        <h3>Diagnostics</h3>

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

        <h4 className="settings-subheading">Actions</h4>
        <Button type="button" variant="secondary" onClick={runDiagnostics}>
          Run diagnostics
        </Button>
        <Button type="button" variant="secondary" onClick={copyDiagnostics}>
          Copy Diagnostics
        </Button>
        {testInStableAvailable ? (
          <>
            <p className="settings-note">
              Pulls origin/main, rebuilds, and restarts this stable Cockpit.
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
