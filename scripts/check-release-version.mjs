// Asserts that every file Release Please bumps carries the same ajax-cli
// version. Release Please writes these four independently (version.txt and the
// manifest by the `simple` strategy, the two TOML files by `extra-files`), so a
// config typo or an upstream parser change silently desynchronises them. Run on
// the release PR head before merge, and locally as part of `npm run verify`.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const PACKAGE = "ajax-cli";

// Deliberately regex, not a TOML parser: these are single, well-known lines and
// a parser would be another dependency for no extra safety.
function tomlPackageVersion(text) {
  const match = /^\s*\[package\][^[]*?^\s*version\s*=\s*"([^"]+)"/ms.exec(text);
  return match?.[1];
}

function lockPackageVersion(text, name) {
  const match = new RegExp(
    String.raw`^\[\[package\]\]\s*^name = "${name}"\s*^version = "([^"]+)"`,
    "m",
  ).exec(text);
  return match?.[1];
}

/** Reads the release version from each file that must agree. */
export function readReleaseVersions(root) {
  const read = (path) => readFileSync(join(root, path), "utf8");

  return {
    ".release-please-manifest.json": JSON.parse(
      read(".release-please-manifest.json"),
    )["."],
    "version.txt": read("version.txt").trim(),
    "crates/ajax-cli/Cargo.toml": tomlPackageVersion(
      read("crates/ajax-cli/Cargo.toml"),
    ),
    "Cargo.lock": lockPackageVersion(read("Cargo.lock"), PACKAGE),
  };
}

/** Returns [] when consistent, otherwise one problem string per bad file. */
export function checkReleaseVersions(root) {
  const versions = readReleaseVersions(root);
  const problems = [];

  for (const [file, version] of Object.entries(versions)) {
    if (!version) {
      problems.push(`${file}: no ${PACKAGE} version found`);
    }
  }

  const distinct = [...new Set(Object.values(versions).filter(Boolean))];

  if (distinct.length > 1) {
    for (const [file, version] of Object.entries(versions)) {
      if (version) {
        problems.push(`${file}: ${version}`);
      }
    }
    problems.unshift(
      `Release version mismatch across ${distinct.length} distinct values:`,
    );
  }

  return problems;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const root = join(fileURLToPath(import.meta.url), "..", "..");
  const problems = checkReleaseVersions(root);

  if (problems.length > 0) {
    for (const problem of problems) {
      console.error(problem);
    }
    process.exit(1);
  }

  console.log(
    `Release version consistent: ${readReleaseVersions(root)["version.txt"]}`,
  );
}
