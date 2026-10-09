/**
 * This fork's own: builds rbxport from this checkout and puts it in
 * /Applications in place of whatever rbxport is there — the official app the
 * first time, which goes to the Trash. The fork never updates itself
 * (`OFFICIAL_FEED` in src-tauri/src/update.rs): run this again after a merge.
 * Run: pnpm app:install            the `fork` profile, a few minutes
 *      pnpm app:install --release  the full release optimisation, for a release
 *
 * `tauri.fork.conf.json` is merged over the release config: only the .app,
 * no updater artifacts (they need upstream's signing key), and an ad-hoc
 * signature so the hardened runtime and its entitlements still apply.
 *
 * The MCP server (crates/rbl-mcp) is built too and goes to
 * ~/.local/bin/rbxport-mcp, where Claude Desktop and Claude Code start it.
 */
import { execFileSync, spawnSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, renameSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const installed = "/Applications/rbxport.app";
const profile = process.argv.includes("--release") ? "release" : "fork";
const built = join(root, "target", profile, "bundle/macos/rbxport.app");

// Tauri builds with the deployment target set to the bundle's minimum macOS;
// SQLCipher's build script reruns whenever it changes, so the server's build
// sets the same one or each build undoes the other's.
const tauriConf = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"));
const env = { ...process.env, MACOSX_DEPLOYMENT_TARGET: tauriConf.bundle?.macOS?.minimumSystemVersion ?? "10.13" };

if (process.platform !== "darwin") {
  throw new Error("pnpm app:install is for macOS.");
}

// `is running` asks Launch Services, not the app: no Automation prompt.
function running() {
  return execFileSync("osascript", ["-e", 'application id "com.rbxport.app" is running'], { encoding: "utf8" }).trim() === "true";
}

if (running()) {
  throw new Error("Quit rbxport first: it cannot be replaced while it runs.");
}

const tauriArgs = ["tauri", "build", "--config", "src-tauri/tauri.fork.conf.json"];
if (profile !== "release") tauriArgs.push("--", "--profile", profile);
const build = spawnSync("pnpm", tauriArgs, { cwd: root, env, stdio: "inherit" });
if (build.status !== 0) process.exit(build.status ?? 1);
if (!existsSync(built)) {
  throw new Error(`The build did not produce ${built}.`);
}

// Asked again: the build takes minutes, long enough to have opened the app.
if (running()) {
  throw new Error("rbxport was opened during the build. Quit it and run pnpm app:install again.");
}

if (existsSync(installed)) {
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const trashed = join(homedir(), ".Trash", `rbxport ${stamp}.app`);
  execFileSync("mv", [installed, trashed]);
  console.log(`The previous rbxport is in the Trash: ${trashed}`);
}
// Moved rather than copied: a second bundle with the same identifier is one
// Launch Services could open in place of this one.
execFileSync("mv", [built, installed]);

const version = execFileSync(
  "/usr/libexec/PlistBuddy",
  ["-c", "Print :CFBundleShortVersionString", join(installed, "Contents/Info.plist")],
  { encoding: "utf8" },
).trim();
console.log(`rbxport ${version} (this fork, ${profile} profile) is installed in ${installed}.`);

const mcp = spawnSync("cargo", ["build", "--profile", profile, "-p", "rbl-mcp"], { cwd: root, env, stdio: "inherit" });
if (mcp.status !== 0) process.exit(mcp.status ?? 1);
const bin = join(homedir(), ".local/bin");
const server = join(bin, "rbxport-mcp");
mkdirSync(bin, { recursive: true });
// Copied beside it and renamed over it: a server a client is running keeps
// its old file rather than having it rewritten under it.
copyFileSync(join(root, "target", profile, "rbxport-mcp"), `${server}.new`);
chmodSync(`${server}.new`, 0o755);
renameSync(`${server}.new`, server);
console.log(`The MCP server is installed in ${server}.`);
