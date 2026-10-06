#!/usr/bin/env node

/**
 * Fetches Git refs, verifies a clean, pushed dev tip descended from main,
 * and dispatches the GitHub Release workflow through gh; it does not build locally.
 * Run: pnpm deploy. Requires authenticated gh and remote access. Optional
 * SKIP_TESTS=true and SKIP_VERSION_BUMP=true control workflow inputs.
 */
import { spawnSync } from "node:child_process";
import { curateReleaseNotes } from "./curate-release-notes.mjs";

function setting(name) {
  const value = process.env[name] ?? "false";
  if (value !== "true" && value !== "false") {
    throw new Error(`${name} must be true or false`);
  }
  return value;
}

function command(program, args, capture = false) {
  const result = spawnSync(program, args, {
    encoding: "utf8",
    stdio: capture ? ["ignore", "pipe", "pipe"] : "inherit",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    const detail = capture ? result.stderr.trim() : "";
    throw new Error(detail || `${program} ${args.join(" ")} failed`);
  }
  return capture ? result.stdout.trim() : "";
}

function requireCondition(condition, message) {
  if (!condition) throw new Error(message);
}

function usage() {
  return `Usage: npm run deploy

Dispatches the Release workflow from the exact clean, pushed dev tip.

Environment:
  SKIP_TESTS=true          Skip release validation tests
  SKIP_VERSION_BUMP=true   Reuse the current version`;
}

function deploy() {
  const skipTests = setting("SKIP_TESTS");
  const skipVersionBump = setting("SKIP_VERSION_BUMP");

  command("git", ["fetch", "origin", "dev", "main", "--tags"]);
  requireCondition(command("git", ["branch", "--show-current"], true) === "dev", "npm run deploy must run from dev");
  requireCondition(command("git", ["status", "--porcelain"], true) === "", "npm run deploy requires a clean working tree");

  const head = command("git", ["rev-parse", "HEAD"], true);
  const remoteDev = command("git", ["rev-parse", "origin/dev"], true);
  requireCondition(head === remoteDev, "local dev is not the pushed dev tip");

  const ancestry = spawnSync("git", ["merge-base", "--is-ancestor", "origin/main", "HEAD"], { stdio: "inherit" });
  if (ancestry.error) throw ancestry.error;
  requireCondition(ancestry.status === 0, "main is not an ancestor of dev");

  let releaseNotesJson = "";
  if (!skipVersionBump) {
    const previous = command("git", ["tag", "--merged", "HEAD", "--list", "v*", "--sort=-version:refname"], true)
      .split("\n").find(Boolean);
    requireCondition(previous, "no previous release tag found");
    console.log(`Asking Codex to curate release notes for ${previous}..${head.slice(0, 7)}.`);
    releaseNotesJson = JSON.stringify(curateReleaseNotes({ previous, source: head }));
  }

  const workflowArguments = [
    "workflow", "run", "Release",
    "--repo", "chrisle/rbxport",
    "--ref", "dev",
    "-f", `skip_tests=${skipTests}`,
    "-f", `skip_version_bump=${skipVersionBump}`,
  ];
  if (releaseNotesJson) workflowArguments.push("-f", `release_notes_json=${releaseNotesJson}`);
  command("gh", workflowArguments);
  console.log(`Release workflow dispatched from ${head.slice(0, 7)}.`);
}

if (process.argv.includes("--help") || process.argv.includes("-h")) {
  console.log(usage());
} else {
  try {
    deploy();
  } catch (error) {
    console.error(`deploy: ${error.message}`);
    process.exitCode = 1;
  }
}
