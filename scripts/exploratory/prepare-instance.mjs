#!/usr/bin/env node
// Prepare an isolated Ajax Web instance tree for exploratory CI.
// Creates config, a disposable git repo, and result directories.
// Does not start the server (workflow / run-agent owns process lifecycle).

import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync, existsSync, rmSync } from "node:fs";
import { join } from "node:path";
import {
  BASE_URL,
  PORT,
  ensureDir,
  instanceDir,
  resultsDir,
  seedResultsSkeleton,
  writeJson,
  repoRoot,
} from "./lib.mjs";

function git(args, cwd) {
  // Husky/git hooks export GIT_DIR / GIT_INDEX_FILE / etc. If inherited, a
  // nested `git -C <sandbox>` still mutates the parent worktree. Strip GIT_*
  // so the disposable demo repo stays isolated even when prepare runs under
  // `git commit` hooks / `npm test`.
  const env = { ...process.env };
  for (const key of Object.keys(env)) {
    if (key.startsWith("GIT_")) delete env[key];
  }
  execFileSync("git", args, { cwd, env, stdio: "pipe" });
}

function prepareRepo(repoPath, bareRemotePath) {
  if (existsSync(repoPath)) {
    rmSync(repoPath, { recursive: true, force: true });
  }
  if (existsSync(bareRemotePath)) {
    rmSync(bareRemotePath, { recursive: true, force: true });
  }
  mkdirSync(repoPath, { recursive: true });
  git(["init", "-b", "main"], repoPath);
  const gitDir = join(repoPath, ".git");
  if (!existsSync(gitDir)) {
    throw new Error(`git init failed to create ${gitDir}`);
  }
  git(["config", "user.email", "exploratory@ajax.local"], repoPath);
  git(["config", "user.name", "Ajax Exploratory"], repoPath);
  writeFileSync(join(repoPath, "README.md"), "# Exploratory demo repo\n");
  git(["add", "README.md"], repoPath);
  git(["commit", "-m", "chore: seed exploratory demo repo"], repoPath);

  // Ajax start plans fetch origin/<default_branch> before worktree add.
  git(["init", "--bare", bareRemotePath], repoPath);
  git(["remote", "add", "origin", bareRemotePath], repoPath);
  git(["push", "-u", "origin", "main"], repoPath);
}

function main() {
  ensureDir(instanceDir);
  ensureDir(join(instanceDir, "worktrees"));
  ensureDir(join(instanceDir, "state"));

  const repoPath = join(instanceDir, "repos", "demo");
  const bareRemotePath = join(instanceDir, "repos", "demo.git");
  prepareRepo(repoPath, bareRemotePath);

  const configPath = join(instanceDir, "config.toml");
  const statePath = join(instanceDir, "state", "ajax.db");
  const worktreeRoot = join(instanceDir, "worktrees");

  writeFileSync(
    configPath,
    `[[repos]]
name = "demo"
path = "${repoPath}"
default_branch = "main"
`,
  );

  const headSha = execFileSync("git", ["rev-parse", "HEAD"], {
    cwd: repoRoot,
    encoding: "utf8",
  }).trim();

  const runMeta = {
    version: 1,
    startedAt: new Date().toISOString(),
    baseUrl: BASE_URL,
    port: PORT,
    headSha,
    instance: {
      config: configPath,
      state: statePath,
      worktreeRoot,
      repo: repoPath,
    },
    infrastructure: {
      status: "prepared",
      error: null,
    },
    agent: {
      status: "pending",
      exitCode: null,
    },
    findingsSummary: {
      confirmed: 0,
      observation: 0,
      rejected: 0,
    },
  };

  seedResultsSkeleton(runMeta);
  writeJson(join(instanceDir, "env.json"), {
    AJAX_EXPLORATORY_BASE_URL: BASE_URL,
    AJAX_EXPLORATORY_PORT: PORT,
    config: configPath,
    state: statePath,
    worktreeRoot,
  });

  writeFileSync(
    join(resultsDir, "logs", "prepare.log"),
    `prepared isolated instance at ${instanceDir}\nbaseUrl=${BASE_URL}\n`,
  );

  console.log(
    JSON.stringify(
      {
        ok: true,
        instanceDir,
        configPath,
        statePath,
        worktreeRoot,
        baseUrl: BASE_URL,
      },
      null,
      2,
    ),
  );
}

main();
