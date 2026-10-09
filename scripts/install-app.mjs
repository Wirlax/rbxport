/**
 * This fork's own: builds rbxport from this checkout and puts it in
 * /Applications in place of whatever rbxport is there — the official app the
 * first time, which goes to the Trash. The fork never updates itself
 * (`OFFICIAL_FEED` in src-tauri/src/update.rs): run this again after a merge.
 * Run: pnpm app:install
 *
 * `tauri.fork.conf.json` is merged over the release config: only the .app,
 * no updater artifacts (they need upstream's signing key), and an ad-hoc
 * signature so the hardened runtime and its entitlements still apply.
 */
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const installed = "/Applications/rbxport.app";
const built = join(root, "target/release/bundle/macos/rbxport.app");

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

const build = spawnSync("pnpm", ["tauri", "build", "--config", "src-tauri/tauri.fork.conf.json"], { cwd: root, stdio: "inherit" });
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
console.log(`rbxport ${version} (this fork) is installed in ${installed}.`);
