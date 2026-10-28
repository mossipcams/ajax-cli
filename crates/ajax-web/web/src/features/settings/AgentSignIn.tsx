import { useCallback, useEffect, useState } from "react";
import * as api from "@/shared/lib/api";
import type { AgentAuthStatus } from "@/shared/lib/api";
import CursorSignIn from "./CursorSignIn";

const HARNESS_LABELS: Record<AgentAuthStatus["harness"], string> = {
  claude: "Claude",
  codex: "Codex",
  cursor: "Cursor",
  pi: "Pi",
};

type AgentAuthState =
  | { kind: "loading" }
  | { kind: "error" }
  | { kind: "ready"; harnesses: AgentAuthStatus[] };

function stateText(status: AgentAuthStatus): string {
  if (!status.installed) return "Not installed";
  if (status.signedIn === true) return "Signed in";
  if (status.signedIn === false) return "Signed out";
  return "Unknown";
}

export default function AgentSignIn() {
  const [state, setState] = useState<AgentAuthState>({ kind: "loading" });

  const refresh = useCallback(() => {
    api.fetchAgentAuthStatus()
      .then((report) => setState({ kind: "ready", harnesses: report.harnesses }))
      .catch(() => setState({ kind: "error" }));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return (
    <div className="settings-section" data-testid="agent-signin-settings">
      <h3>Agents</h3>
      {state.kind === "loading" ? <p className="settings-status">Checking sign-in…</p> : null}
      {state.kind === "error" ? <p className="settings-status">Could not check sign-in.</p> : null}
      {state.kind === "ready" ? (
        <>
          <dl className="settings-debug" data-testid="agent-signin-list">
            {state.harnesses.map((status) => (
              <div key={status.harness} data-testid={`agent-signin-${status.harness}`}>
                <dt>{HARNESS_LABELS[status.harness]}</dt>
                <dd>
                  {stateText(status)}
                  {status.detail ? (
                    <>
                      <br />
                      <span className="settings-note">{status.detail}</span>
                    </>
                  ) : null}
                </dd>
              </div>
            ))}
          </dl>
          <CursorSignIn onSignedIn={refresh} />
        </>
      ) : null}
    </div>
  );
}
