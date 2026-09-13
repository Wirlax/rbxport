//! Link export in the app: LINK on and off, and what the players are doing.
//!
//! `rbl-link` owns the protocol; this is the glue — the library it reads is
//! the app's, reloaded or not, through a weak handle to the state, and the
//! players it hears are reported to the window as an event whenever they
//! change, no more than twice a second.
//!
//! LINK is refused while rekordbox runs: it holds every port a player looks
//! for, and two sources called `rekordbox` on one network would be worse
//! than one that says why it stays off.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use rbl_link::{Interface, LinkExport, Ports, Snapshot, Source};
use serde::Serialize;

use crate::state::AppState;

/// How often the players are looked at for a change worth reporting.
const REPORT_EVERY: Duration = Duration::from_millis(500);

/// A network interface LINK can run on, as the interface offers it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceDto {
    pub name: String,
    pub address: String,
}

impl From<&Interface> for InterfaceDto {
    fn from(interface: &Interface) -> Self {
        Self { name: interface.name.clone(), address: interface.address.to_string() }
    }
}

/// A track a player has loaded from us, named for the window.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LoadedDto {
    pub id: String,
    pub title: String,
    pub artist: String,
}

/// A player on the link, as the window shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlayerDto {
    pub number: u8,
    pub name: String,
    pub kind: String,
    pub address: String,
    pub loaded: Option<LoadedDto>,
    pub playing: bool,
    pub master: bool,
}

/// Whether LINK is on, on what, and who is listening.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LinkStatusDto {
    pub on: bool,
    /// Why it could not be turned on, when it could not.
    pub problem: Option<String>,
    pub interface: Option<InterfaceDto>,
    pub players: Vec<PlayerDto>,
    /// What LINK could run on, for the picker.
    pub interfaces: Vec<InterfaceDto>,
}

impl LinkStatusDto {
    pub fn off(problem: Option<String>) -> Self {
        Self { on: false, problem, interface: None, players: Vec::new(), interfaces: interfaces() }
    }
}

/// The interfaces on offer, as the window lists them.
pub fn interfaces() -> Vec<InterfaceDto> {
    rbl_link::interfaces().iter().map(InterfaceDto::from).collect()
}

/// The app's library, as the link reads it. Weak so the state does not own
/// a session that owns the state.
struct StateSource(Weak<AppState>);

impl Source for StateSource {
    fn library(&self) -> Option<Arc<rbl_index::Library>> {
        self.0.upgrade()?.library().ok()
    }

    fn share_root(&self) -> std::path::PathBuf {
        self.0.upgrade().map(|state| state.share_root()).unwrap_or_default()
    }

    fn details(&self, id: &str) -> Option<rbl_db::details::TrackDetails> {
        let state = self.0.upgrade()?;
        state.read_db(|db| rbl_db::details::track_details(db.connection(), id)).ok().flatten()
    }
}

/// A running LINK session: the export, and the thread that reports it.
pub struct Session {
    export: Option<LinkExport>,
    stop: Arc<AtomicBool>,
    reporter: Option<std::thread::JoinHandle<()>>,
}

impl Session {
    /// Turns LINK on for `interface`, or the first interface when none is
    /// named, and reports the players to `report` as they change.
    ///
    /// Blocking: binds seven sockets and walks every track's path.
    pub fn start<F>(state: &Arc<AppState>, interface: Option<&str>, report: F) -> Result<Self, String>
    where
        F: Fn(LinkStatusDto) + Send + 'static,
    {
        let available = rbl_link::interfaces();
        let chosen = match interface {
            Some(name) => available.iter().find(|i| i.name == name).cloned(),
            None => available.first().cloned(),
        }
        .ok_or_else(|| match interface {
            Some(name) => format!("No network interface called {name}."),
            None => "No network interface to run LINK on.".to_owned(),
        })?;

        let source: Arc<dyn Source> = Arc::new(StateSource(Arc::downgrade(state)));
        let export = LinkExport::start(source, chosen, Ports::REKORDBOX).map_err(|e| e.to_string())?;

        let stop = Arc::new(AtomicBool::new(false));
        let weak = Arc::downgrade(state);
        let reporter = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut last: Option<Vec<PlayerDto>> = None;
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(REPORT_EVERY);
                    let Some(state) = weak.upgrade() else { return };
                    let Some(status) = state.link_status() else { return };
                    if last.as_ref() != Some(&status.players) {
                        last = Some(status.players.clone());
                        report(status);
                    }
                }
            })
        };
        Ok(Self { export: Some(export), stop, reporter: Some(reporter) })
    }

    /// The session as the window shows it, with the loaded tracks named
    /// from `library`. The library is an argument rather than read from the
    /// state here: the caller holds the state's lock already.
    pub fn status(&self, library: Option<&rbl_index::Library>) -> LinkStatusDto {
        let Some(export) = &self.export else {
            return LinkStatusDto::off(None);
        };
        let snapshot = export.snapshot();
        LinkStatusDto {
            on: true,
            problem: None,
            interface: Some(InterfaceDto::from(&snapshot.interface)),
            players: players(library, &snapshot),
            interfaces: interfaces(),
        }
    }

    /// After an analysis run: the players get fresh files.
    pub fn analysis_changed(&self) {
        if let Some(export) = &self.export {
            export.analysis_changed();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(export) = self.export.take() {
            export.stop();
        }
        if let Some(reporter) = self.reporter.take() {
            drop(reporter.join());
        }
    }
}

/// The players with their loaded tracks named from the library.
fn players(library: Option<&rbl_index::Library>, snapshot: &Snapshot) -> Vec<PlayerDto> {
    snapshot
        .players
        .iter()
        .map(|player| PlayerDto {
            number: player.number,
            name: player.name.clone(),
            kind: match player.kind {
                rbl_link::DeviceType::Cdj => "player",
                rbl_link::DeviceType::Mixer => "mixer",
                rbl_link::DeviceType::Rekordbox => "rekordbox",
                rbl_link::DeviceType::Other(_) => "device",
            }
            .to_owned(),
            address: player.address.to_string(),
            loaded: player.loaded.map(|id| {
                let row = library.and_then(|l| l.row_of_id(u64::from(id)));
                LoadedDto {
                    id: id.to_string(),
                    title: row.zip(library).map(|(r, l)| l.title.get(r as usize).to_owned()).unwrap_or_default(),
                    artist: row.zip(library).map(|(r, l)| l.artist_name(r).to_owned()).unwrap_or_default(),
                }
            }),
            playing: player.playing,
            master: player.master,
        })
        .collect()
}

/// Why LINK cannot start now, if it cannot: rekordbox holds the ports.
pub fn refusal() -> Option<String> {
    if rbl_db::is_rekordbox_running() {
        return Some("rekordbox is running and holds the link ports. Quit it to turn LINK on.".to_owned());
    }
    None
}
