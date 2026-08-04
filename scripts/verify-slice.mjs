#!/usr/bin/env node
/**
 * Fast, slice-local verification for agentic edits.
 *
 * Usage:
 *   node scripts/verify-slice.mjs <name>
 *   npm run verify:slice -- repair
 *
 * Names:
 *   start|resume|review|repair|ship|drop_task|sweep_cleanup  → ajax-core slice
 *   operate|cockpit|terminal|install                         → ajax-web slice
 *   cli|core|web|arch|supervisor|tui                         → crate / arch suite
 */

import { spawnSync } from "node:child_process";

const name = process.argv[2];
if (!name) {
  console.error("usage: verify-slice <name>");
  process.exit(2);
}

const CORE_SLICES = new Set([
  "start",
  "resume",
  "review",
  "repair",
  "ship",
  "drop_task",
  "sweep_cleanup",
]);

const WEB_SLICES = new Set(["operate", "cockpit", "terminal", "install"]);

function run(cmd, args) {
  console.log(`+ ${cmd} ${args.join(" ")}`);
  const result = spawnSync(cmd, args, { stdio: "inherit", shell: false });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function cargoCheck(pkg) {
  run("cargo", ["check", "-p", pkg, "--all-features"]);
}

function nextest(pkg, filter) {
  const args = ["nextest", "run", "-p", pkg, "--all-features"];
  if (filter) {
    args.push("-E", filter);
  }
  run("cargo", args);
}

function architectureTests(pkg) {
  run("cargo", ["test", "-p", pkg, "architecture", "--", "--nocapture"]);
}

if (name === "arch") {
  for (const pkg of ["ajax-core", "ajax-web", "ajax-tui", "ajax-supervisor"]) {
    architectureTests(pkg);
  }
  process.exit(0);
}

if (name === "core") {
  cargoCheck("ajax-core");
  nextest("ajax-core");
  process.exit(0);
}

if (name === "cli") {
  cargoCheck("ajax-cli");
  nextest("ajax-cli");
  process.exit(0);
}

if (name === "web") {
  cargoCheck("ajax-web");
  nextest("ajax-web");
  process.exit(0);
}

if (name === "tui") {
  cargoCheck("ajax-tui");
  nextest("ajax-tui");
  process.exit(0);
}

if (name === "supervisor") {
  cargoCheck("ajax-supervisor");
  nextest("ajax-supervisor");
  process.exit(0);
}

if (CORE_SLICES.has(name)) {
  cargoCheck("ajax-core");
  architectureTests("ajax-core");
  // Match the slice name in test paths/binary filters; also run task_operations suite.
  nextest(
    "ajax-core",
    `test(task_operations) | test(${name}) | test(architecture)`,
  );
  process.exit(0);
}

if (WEB_SLICES.has(name)) {
  cargoCheck("ajax-web");
  architectureTests("ajax-web");
  nextest("ajax-web", `test(${name}) | test(architecture)`);
  process.exit(0);
}

console.error(
  `unknown slice/area '${name}'. Known: ${[
    ...CORE_SLICES,
    ...WEB_SLICES,
    "cli",
    "core",
    "web",
    "tui",
    "supervisor",
    "arch",
  ].join(", ")}`,
);
process.exit(2);
