// Deterministic mission catalog and selection for nightly exploratory runs.

import { FINDING_AREAS, MISSION_COOLDOWN_RUNS, MISSION_NO_SIGNAL_COOLDOWN_MS } from "./lib.mjs";

export const MISSIONS = [
  {
    id: "garbage-hashes",
    charter: "Garbage hashes",
    area: "navigation",
    pathHints: ["route", "hash", "navigation", "banner", "404", "redirect"],
    commitHints: ["route", "hash", "nav", "banner"],
    needsFakeAcp: false,
    seed: null,
    avoidFingerprints: ["navigation|hash-garbage", "navigation|banner-stuck"],
  },
  {
    id: "happy-path-session",
    charter: "Happy path",
    area: "session",
    pathHints: ["session", "chat", "acp", "composer", "message"],
    commitHints: ["session", "chat", "acp", "composer"],
    needsFakeAcp: true,
    seed: { kind: "task", repo: "demo", title: "exploratory-test-task-alpha", agent: "cursor" },
    avoidFingerprints: [],
  },
  {
    id: "interruption-recovery",
    charter: "Interruption",
    area: "cockpit",
    pathHints: ["cockpit", "dashboard", "reload", "retry"],
    commitHints: ["cockpit", "dashboard", "reload"],
    needsFakeAcp: true,
    seed: { kind: "task", repo: "demo", title: "exploratory-test-task-beta", agent: "cursor" },
    avoidFingerprints: [],
  },
  {
    id: "contradiction-banner",
    charter: "Contradiction",
    area: "network",
    pathHints: ["banner", "health", "connection", "disconnected"],
    commitHints: ["banner", "health", "connection"],
    needsFakeAcp: false,
    seed: null,
    avoidFingerprints: ["network|disconnected-banner"],
  },
  {
    id: "recovery-empty",
    charter: "Recovery",
    area: "new-task",
    pathHints: ["start", "validation", "empty", "new-task", "create"],
    commitHints: ["start", "create", "validation"],
    needsFakeAcp: false,
    seed: null,
    avoidFingerprints: [],
  },
  {
    id: "settings-diagnostics",
    charter: "Recovery",
    area: "settings",
    pathHints: ["settings", "diagnostics", "preference"],
    commitHints: ["settings", "diagnostic"],
    needsFakeAcp: false,
    seed: null,
    avoidFingerprints: [],
  },
  {
    id: "diff-review",
    charter: "Happy path",
    area: "diff-review",
    pathHints: ["diff", "review", "pull", "pr"],
    commitHints: ["diff", "review", "pull"],
    needsFakeAcp: true,
    seed: { kind: "task", repo: "demo", title: "exploratory-test-task-alpha", agent: "cursor" },
    avoidFingerprints: [],
  },
  {
    id: "terminal-input",
    charter: "Happy path",
    area: "terminal",
    pathHints: ["terminal", "xterm", "paste", "scroll"],
    commitHints: ["terminal", "xterm", "paste"],
    needsFakeAcp: true,
    seed: { kind: "task", repo: "demo", title: "exploratory-test-task-alpha", agent: "cursor" },
    avoidFingerprints: [],
  },
];

function knownFingerprints(memory) {
  return new Set((memory?.confirmedFindings ?? []).map((item) => item.fingerprint).filter(Boolean));
}

function missionLastRunAt(memory, missionId) {
  const entry = memory?.missions?.[missionId];
  if (!entry?.lastRunAt) return 0;
  const parsed = Date.parse(entry.lastRunAt);
  return Number.isFinite(parsed) ? parsed : 0;
}

function missionOnCooldown(entry, now = Date.now()) {
  if (!entry?.cooldownUntil) return false;
  const until = Date.parse(entry.cooldownUntil);
  return Number.isFinite(until) && until > now;
}

function scoreMission(mission, { changedPaths, changedCommits, memory }) {
  const text = `${changedPaths.join("\n")}\n${changedCommits.join("\n")}`.toLowerCase();
  let changeScore = 0;
  for (const hint of mission.pathHints) {
    if (changedPaths.some((path) => path.toLowerCase().includes(hint))) changeScore += 3;
  }
  for (const hint of mission.commitHints) {
    if (text.includes(hint)) changeScore += 2;
  }
  const known = knownFingerprints(memory);
  let penalty = 0;
  for (const fingerprint of mission.avoidFingerprints ?? []) {
    if (known.has(fingerprint)) penalty += 10;
  }
  const history = memory?.missions?.[mission.id];
  if ((history?.noSignalCount ?? 0) >= MISSION_COOLDOWN_RUNS) {
    penalty += 4;
  }
  return changeScore - penalty;
}

export function selectMission({
  memory = {},
  headSha = null,
  changedPaths = [],
  changedCommits = [],
  now = Date.now(),
} = {}) {
  const candidates = MISSIONS.filter((mission) => !missionOnCooldown(memory?.missions?.[mission.id], now)).map(
    (mission) => ({
      mission,
      changeScore: scoreMission(mission, { changedPaths, changedCommits, memory }),
      lastRunAt: missionLastRunAt(memory, mission.id),
      runs: memory?.missions?.[mission.id]?.runs ?? 0,
    }),
  );

  candidates.sort((a, b) => {
    if (b.changeScore !== a.changeScore) return b.changeScore - a.changeScore;
    if (a.lastRunAt !== b.lastRunAt) return a.lastRunAt - b.lastRunAt;
    if (a.runs !== b.runs) return a.runs - b.runs;
    return a.mission.id.localeCompare(b.mission.id);
  });

  const primary = candidates[0]?.mission ?? MISSIONS[0];
  const fallback =
    candidates.find((item) => item.mission.id !== primary.id)?.mission ??
    MISSIONS.find((mission) => mission.id !== primary.id) ??
    primary;

  return {
    version: 1,
    headSha,
    sinceSha: memory?.lastRunSha ?? null,
    selectedAt: new Date().toISOString(),
    primary: summarizeMission(primary),
    fallback: summarizeMission(fallback),
    rationale: {
      changeScorePrimary: scoreMission(primary, { changedPaths, changedCommits, memory }),
      changeScoreFallback: scoreMission(fallback, { changedPaths, changedCommits, memory }),
      changedPathCount: changedPaths.length,
      changedCommitCount: changedCommits.length,
    },
  };
}

function summarizeMission(mission) {
  return {
    id: mission.id,
    charter: mission.charter,
    area: mission.area,
    needsFakeAcp: mission.needsFakeAcp,
    seed: mission.seed,
  };
}

export function emptyMissionMemory() {
  const missions = {};
  for (const mission of MISSIONS) {
    missions[mission.id] = {
      runs: 0,
      lastRunAt: null,
      lastRunSha: null,
      lastOutcome: null,
      noSignalCount: 0,
      cooldownUntil: null,
    };
  }
  return missions;
}

export function touchMissionMemory(memory, missionId, sha, at = new Date().toISOString(), outcome = "completed") {
  if (!memory.missions) memory.missions = emptyMissionMemory();
  if (!memory.missions[missionId]) {
    memory.missions[missionId] = {
      runs: 0,
      lastRunAt: null,
      lastRunSha: null,
      lastOutcome: null,
      noSignalCount: 0,
      cooldownUntil: null,
    };
  }
  const entry = memory.missions[missionId];
  entry.runs = (entry.runs ?? 0) + 1;
  entry.lastRunAt = at;
  entry.lastRunSha = sha;
  entry.lastOutcome = outcome;
  if (outcome === "no-signal") {
    entry.noSignalCount = (entry.noSignalCount ?? 0) + 1;
    if (entry.noSignalCount >= MISSION_COOLDOWN_RUNS) {
      entry.cooldownUntil = new Date(Date.parse(at) + MISSION_NO_SIGNAL_COOLDOWN_MS).toISOString();
    }
  } else {
    entry.noSignalCount = 0;
    entry.cooldownUntil = null;
  }
  return memory;
}

export function missionArea(missionId) {
  const mission = MISSIONS.find((item) => item.id === missionId);
  if (!mission) return "other";
  return FINDING_AREAS.has(mission.area) ? mission.area : "other";
}

export function resolveMissionOutcome({ run = {}, memoryDelta = {}, findings = { findings: [] }, observations = { observations: [] } } = {}) {
  if (run.agent?.status === "failed" || run.infrastructure?.status === "failed") {
    return "blocked";
  }
  const findingCount = findings.findings?.length ?? 0;
  const observationCount =
    (observations.observations ?? []).length +
    (findings.findings ?? []).filter((item) => item.status === "observation").length;
  const areas = (memoryDelta.areasVisited ?? []).length;
  if (findingCount === 0 && observationCount === 0 && areas === 0) {
    return "no-signal";
  }
  if (memoryDelta.missionCompleted === true || areas > 0 || findingCount > 0 || observationCount > 0) {
    return "completed";
  }
  return "partial";
}
