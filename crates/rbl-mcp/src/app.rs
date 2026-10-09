//! rbxport, driven over AppleScript: launched when a write needs it, and
//! quit once idle if this server was the one that launched it, so it never
//! runs all day unasked.
//!
//! Every write goes through the app's own `place mini sets` command rather
//! than to the database: the app holds the edit gate, refreshes its index and
//! redraws the playlist, and refuses while rekordbox runs.

use std::process::Output;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::process::Command;

pub const BUNDLE_ID: &str = "com.rbxport.app";
const APP_PATH: &str = "/Applications/rbxport.app";
/// How long a launch may take to have the library loaded.
const READY_TIMEOUT: Duration = Duration::from_secs(90);
/// How long one AppleScript round may take before it is given up on.
const SCRIPT_TIMEOUT: Duration = Duration::from_secs(150);

/// Where the blocks go, resolved: a playlist id, or a new playlist's name.
pub enum Destination {
    Playlist(String),
    New(String),
}

#[derive(Debug)]
struct Launch {
    /// This server launched the running app, so it may quit it.
    ours: bool,
    last_write: Instant,
}

#[derive(Debug)]
pub struct App {
    launch: Mutex<Launch>,
    /// One write at a time: two would both see the app closed and launch it.
    writing: tokio::sync::Mutex<()>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self { launch: Mutex::new(Launch { ours: false, last_write: Instant::now() }), writing: tokio::sync::Mutex::new(()) }
    }

    /// Places the blocks through the app and answers the separator each went
    /// after. The ids must be digits: they are written into the script.
    pub async fn place(&self, destination: &Destination, blocks: &[Vec<String>]) -> Result<Vec<String>, String> {
        let _one = self.writing.lock().await;
        self.ensure_running().await?;
        let (script, argument) = place_script(destination, blocks)?;
        let placed = osascript(&script, &[argument.as_str()]).await;
        self.with_launch(|l| l.last_write = Instant::now());
        Ok(list_items(&placed?))
    }

    /// Quits the app if this server launched it and nothing was written for
    /// `idle` — unless it is the app in front, which means Ronan is using it.
    pub async fn quit_if_idle(&self, idle: Duration) {
        let due = self.with_launch(|l| l.ours && l.last_write.elapsed() >= idle);
        if due {
            self.quit_ours().await;
        }
    }

    /// On the way out: the app goes too if this server launched it.
    pub async fn quit_if_ours(&self) {
        if self.with_launch(|l| l.ours) {
            self.quit_ours().await;
        }
    }

    async fn quit_ours(&self) {
        if !is_running().await.unwrap_or(false) {
            self.with_launch(|l| l.ours = false);
            return;
        }
        if is_frontmost().await {
            return;
        }
        if osascript(&format!("tell application id \"{BUNDLE_ID}\" to quit"), &[]).await.is_ok() {
            self.with_launch(|l| l.ours = false);
        }
    }

    fn with_launch<T>(&self, f: impl FnOnce(&mut Launch) -> T) -> T {
        let mut launch = self.launch.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut launch)
    }

    async fn ensure_running(&self) -> Result<(), String> {
        if !is_running().await? {
            if !std::path::Path::new(APP_PATH).is_dir() {
                return Err(format!("rbxport is not in {APP_PATH}: install it with `pnpm app:install` from the fork."));
            }
            // In the background, so the conversation keeps the front.
            let opened = Command::new("open").args(["-g", "-a", APP_PATH]).output().await;
            check(opened, "open rbxport")?;
            self.with_launch(|l| {
                l.ours = true;
                l.last_write = Instant::now();
            });
        }
        wait_ready().await
    }
}

/// The app answers scripts and has the library loaded: it counts tracks.
/// Before the load finishes it counts none.
async fn wait_ready() -> Result<(), String> {
    let deadline = Instant::now() + READY_TIMEOUT;
    let count = format!("tell application id \"{BUNDLE_ID}\" to count tracks");
    loop {
        if let Ok(n) = osascript(&count, &[]).await {
            if n.trim().parse::<u64>().is_ok_and(|n| n > 0) {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            return Err("rbxport did not finish loading the library in time.".to_owned());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn is_running() -> Result<bool, String> {
    // `is running` asks Launch Services, not the app: no Automation prompt.
    let answer = osascript(&format!("application id \"{BUNDLE_ID}\" is running"), &[]).await?;
    Ok(answer == "true")
}

/// Whether rbxport is the frontmost app, asked of Launch Services.
async fn is_frontmost() -> bool {
    let Ok(front) = Command::new("lsappinfo").arg("front").output().await else { return false };
    let asn = String::from_utf8_lossy(&front.stdout).trim().to_owned();
    if asn.is_empty() {
        return false;
    }
    let Ok(info) = Command::new("lsappinfo").args(["info", "-only", "bundleid", &asn]).output().await else { return false };
    String::from_utf8_lossy(&info.stdout).contains(BUNDLE_ID)
}

/// The script for one write, and the argument it takes: the playlist id or
/// the new playlist's name, passed apart so a name needs no escaping.
fn place_script(destination: &Destination, blocks: &[Vec<String>]) -> Result<(String, String), String> {
    if let Some(bad) = blocks.iter().flatten().find(|id| id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit())) {
        return Err(format!("{bad:?} is not a track id."));
    }
    let list = blocks
        .iter()
        .map(|block| format!("{{{}}}", block.iter().map(|id| format!("\"{id}\"")).collect::<Vec<_>>().join(", ")))
        .collect::<Vec<_>>()
        .join(", ");
    let (target, argument) = match destination {
        Destination::Playlist(id) => ("to playlist id (item 1 of argv)", id.clone()),
        Destination::New(name) => ("creating playlist (item 1 of argv)", name.clone()),
    };
    let script = format!("on run argv\n\ttell application id \"{BUNDLE_ID}\" to place mini sets {{{list}}} {target}\nend run");
    Ok((script, argument))
}

async fn osascript(script: &str, arguments: &[&str]) -> Result<String, String> {
    let run = Command::new("osascript").arg("-e").arg(script).args(arguments).kill_on_drop(true).output();
    let output = tokio::time::timeout(SCRIPT_TIMEOUT, run)
        .await
        .map_err(|_| "rbxport did not answer in time.".to_owned())?;
    check(output, "talk to rbxport")
}

fn check(output: std::io::Result<Output>, doing: &str) -> Result<String, String> {
    let output = output.map_err(|e| format!("Could not {doing}: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(script_error(&String::from_utf8_lossy(&output.stderr)))
    }
}

/// The app's own words out of osascript's report:
/// `…: execution error: rbxport got an error: rekordbox is running. Quit it before making changes. (-10004)`.
fn script_error(stderr: &str) -> String {
    let text = stderr.trim();
    let text = text.rsplit_once("execution error: ").map_or(text, |(_, rest)| rest);
    let text = text.strip_prefix("rbxport got an error: ").unwrap_or(text);
    let text = match text.rfind(" (-") {
        Some(at) if text.ends_with(')') => text.get(..at).unwrap_or(text),
        _ => text,
    };
    text.trim().to_owned()
}

/// An AppleScript list of text as osascript prints it: `a, b, c`.
fn list_items(printed: &str) -> Vec<String> {
    printed.split(", ").map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_apps_words_come_out_of_osascripts_report() {
        let stderr = "34:120: execution error: rbxport got an error: rekordbox is running. Quit it before making changes. (-10004)\n";
        assert_eq!(script_error(stderr), "rekordbox is running. Quit it before making changes.");
        assert_eq!(script_error("plain failure"), "plain failure");
    }

    #[test]
    fn the_script_lists_the_blocks_and_takes_the_target_apart() {
        let blocks = vec![vec!["12".to_owned(), "34".to_owned()], vec!["56".to_owned()]];
        let (script, argument) = place_script(&Destination::New("Mon \"set\"".to_owned()), &blocks).unwrap();
        assert!(script.contains("place mini sets {{\"12\", \"34\"}, {\"56\"}} creating playlist (item 1 of argv)"), "{script}");
        assert_eq!(argument, "Mon \"set\"");
        let (script, argument) = place_script(&Destination::Playlist("99".to_owned()), &blocks).unwrap();
        assert!(script.contains("to playlist id (item 1 of argv)"));
        assert_eq!(argument, "99");
        assert!(place_script(&Destination::Playlist("99".to_owned()), &[vec!["1\"; quit".to_owned()]]).is_err());
    }

    #[test]
    fn a_printed_list_splits_into_its_items() {
        assert_eq!(list_items("SEPARATORBREMSEN, SEPARATORBREMSEN 100"), ["SEPARATORBREMSEN", "SEPARATORBREMSEN 100"]);
        assert_eq!(list_items(""), Vec::<String>::new());
    }
}
