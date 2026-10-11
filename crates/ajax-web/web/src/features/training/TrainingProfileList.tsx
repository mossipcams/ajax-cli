import { Button } from "@/shared/ui/button";

import * as trainingApi from "./trainingApi";

export interface TrainingProfileListProps {
  runtimeUp: boolean;
  activeProfile: string | null;
  busy: boolean;
  switchBlocked: string | null;
  switchingKind: "switch" | null;
  switchingProfile: string | null;
  profiles: string[];
  details: trainingApi.TrainingProfileDetails | null;
  profilesError: string | null;
  onServe: () => void;
  onServeStop: () => void;
  onSwitch: (profile: string) => void;
}

function renderLlamaState(runtimeUp: boolean, activeProfile: string | null) {
  return (
    <p className="training-muted" data-testid="llama-state">
      {runtimeUp ? "Serving" : "Stopped"}
      {activeProfile ? ` · ${activeProfile}` : ""}
    </p>
  );
}

function renderSwitchBlocked(switchBlocked: string | null) {
  if (!switchBlocked) return null;
  return (
    <p className="training-muted training-switch-blocked">
      Switching is unavailable while {switchBlocked}.
    </p>
  );
}

export default function TrainingProfileList({
  runtimeUp,
  activeProfile,
  busy,
  switchBlocked,
  switchingKind,
  switchingProfile,
  profiles,
  details,
  profilesError,
  onServe,
  onServeStop,
  onSwitch,
}: TrainingProfileListProps) {
  return (
    <section className="training-section" aria-labelledby="training-models-heading">
      <h3 id="training-models-heading">llama.cpp &amp; profiles</h3>
      {renderLlamaState(runtimeUp, activeProfile)}
      <div className="training-llama-actions">
        <Button type="button" variant="secondary" disabled={busy || runtimeUp || !activeProfile} onClick={onServe}>
          Start serving
        </Button>
        <Button type="button" variant="secondary" disabled={busy || !runtimeUp} onClick={onServeStop}>
          Stop serving
        </Button>
      </div>

      <ul className="training-profile-list">
        {profiles.map((name) => {
          const detail = details?.[name] ?? {};
          const isActive = name === activeProfile;
          const switching = switchingKind === "switch" && switchingProfile === name;
          return (
            <li key={name} className={`training-profile${isActive ? " is-active" : ""}`}>
              {switching ? (
                <p className="training-muted training-switch-busy" data-testid="training-switch-busy">
                  Switching to {detail.label ?? name}…
                </p>
              ) : (
                <button
                  type="button"
                  className="training-profile-picker"
                  aria-pressed={isActive}
                  disabled={isActive || busy || Boolean(switchBlocked)}
                  onClick={() => onSwitch(name)}
                >
                  <span className="training-profile-text">
                    <strong>{detail.label ?? name}</strong>
                    {isActive ? (
                      <em className="training-tag">active</em>
                    ) : null}
                    {isActive ? (
                      runtimeUp ? (
                        <em className="training-tag">serving</em>
                      ) : (
                        <em className="training-tag is-muted">stopped</em>
                      )
                    ) : null}
                  </span>
                  {detail.serving && isActive ? (
                    <span className="training-serving">{detail.serving}</span>
                  ) : null}
                  {detail.model ? (
                    <span className="training-model" title={detail.model}>
                      {detail.model}
                    </span>
                  ) : null}
                </button>
              )}
            </li>
          );
        })}
      </ul>

      {profilesError ? (
        <p role="alert" className="training-banner" data-testid="training-profiles-error">
          Model list unavailable: {profilesError}
        </p>
      ) : null}
      {renderSwitchBlocked(switchBlocked)}
    </section>
  );
}
