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

/// A device heard on the network before LINK is on: enough to say "a player
/// is here", not what it has loaded.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PeerDto {
    pub number: u8,
    pub name: String,
    /// `player`, `mixer`, `rekordbox` or `device`.
    pub kind: String,
    pub address: String,
}

/// How a device type reads for the window.
fn kind_name(kind: rbl_link::DeviceType) -> &'static str {
    match kind {
        rbl_link::DeviceType::Cdj => "player",
        rbl_link::DeviceType::Mixer => "mixer",
        rbl_link::DeviceType::Rekordbox => "rekordbox",
        rbl_link::DeviceType::Other(_) => "device",
    }
}

impl PeerDto {
    fn from_player(player: &rbl_link::Player) -> Self {
        Self {
            number: player.number,
            name: player.name.clone(),
            kind: kind_name(player.kind).to_owned(),
            address: player.address.to_string(),
        }
    }
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
    /// The player has mounted the library: a track can be sent to it.
    /// rekordbox refuses a drag to a player until then.
    pub mounted: bool,
}

/// Whether LINK is on, on what, and who is listening.
// Not `Eq`: `master_bpm` is an `f64`. Callers compare `players`, which is.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LinkStatusDto {
    pub on: bool,
    /// Why it could not be turned on, when it could not.
    pub problem: Option<String>,
    pub interface: Option<InterfaceDto>,
    pub players: Vec<PlayerDto>,
    /// What LINK could run on, for the picker.
    pub interfaces: Vec<InterfaceDto>,
    /// We are the network's tempo master, driving the tempo the players sync
    /// to.
    pub master: bool,
    /// The master tempo we would drive, in BPM. Shown on the master control
    /// whether or not we are master, as rekordbox shows the last value.
    pub master_bpm: f64,
    /// Where the join is while on: `waiting` (nothing announced until a
    /// player or mixer is heard, as rekordbox does), `joining` (probing for
    /// a device number), `up`, or `down` (the interface lost its address;
    /// `problem` says so). `off` while off.
    pub state: String,
    /// The device number the join settled on — 17, or 18 when another
    /// rekordbox holds 17 — once it has.
    pub number: Option<u8>,
}

impl LinkStatusDto {
    pub fn off(problem: Option<String>) -> Self {
        Self {
            on: false,
            problem,
            interface: None,
            players: Vec::new(),
            interfaces: interfaces(),
            master: false,
            master_bpm: 120.0,
            state: "off".to_owned(),
            number: None,
        }
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
    /// Turns LINK on for `interface`, or when none is named the interface
    /// the OS reaches a player already heard through — the first interface
    /// when no player has been heard yet — and reports the players to
    /// `report` as they change.
    ///
    /// Blocking: binds seven sockets and walks every track's path.
    pub fn start<F>(state: &Arc<AppState>, interface: Option<&str>, report: F) -> Result<Self, String>
    where
        F: Fn(LinkStatusDto) + Send + 'static,
    {
        let available = rbl_link::interfaces();
        tracing::debug!(
            interfaces = ?available.iter().map(|i| format!("{} {}/{}", i.name, i.address, i.netmask)).collect::<Vec<_>>(),
            "interfaces LINK could run on"
        );
        let chosen = if let Some(name) = interface {
            available.iter().find(|i| i.name == name).cloned()
        } else {
            let peers = state.link_peers();
            let toward = peers
                .iter()
                .find_map(|peer| rbl_link::interface_toward(&available, peer.address).map(|i| (peer.address, i)));
            if let Some((peer, i)) = toward {
                tracing::debug!(interface = %i.name, %peer, "interface chosen: the one that reaches a device already heard");
                Some(i)
            } else {
                tracing::debug!(peers = peers.len(), "no device heard on any interface; taking the first");
                available.first().cloned()
            }
        }
        .ok_or_else(|| match interface {
            Some(name) => format!("No network interface called {name}."),
            None => "No network interface to run LINK on.".to_owned(),
        })?;
        tracing::info!(interface = %chosen.name, address = %chosen.address, "LINK running on an interface");

        let source: Arc<dyn Source> = Arc::new(StateSource(Arc::downgrade(state)));
        let export = LinkExport::start(source, chosen, Ports::REKORDBOX).map_err(|e| e.to_string())?;

        let stop = Arc::new(AtomicBool::new(false));
        let weak = Arc::downgrade(state);
        let reporter = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut last: Option<(String, Option<u8>, Vec<PlayerDto>)> = None;
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(REPORT_EVERY);
                    let Some(state) = weak.upgrade() else { return };
                    let Some(status) = state.link_status() else { return };
                    // The link went down — the interface lost its address,
                    // or no device number could be had: LINK goes off with
                    // the reason, as rekordbox's LINKDOWN takes its button
                    // back to grey. The session is dropped on a thread of
                    // its own, since dropping it joins this one.
                    if status.state == "down" {
                        tracing::warn!(problem = status.problem.as_deref().unwrap_or(""), "the link went down; LINK off");
                        let session = state.set_link(None);
                        std::thread::spawn(move || drop(session));
                        report(LinkStatusDto::off(status.problem));
                        return;
                    }
                    let now = (status.state.clone(), status.number, status.players.clone());
                    if last.as_ref() != Some(&now) {
                        last = Some(now);
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
        let (state, number, problem) = match &snapshot.link {
            rbl_link::LinkState::Waiting => ("waiting", None, None),
            rbl_link::LinkState::Joining => ("joining", None, None),
            rbl_link::LinkState::Up { number } => ("up", Some(*number), None),
            rbl_link::LinkState::Down(why) => ("down", None, Some(why.clone())),
        };
        LinkStatusDto {
            on: true,
            problem,
            interface: Some(InterfaceDto::from(&snapshot.interface)),
            players: players(library, &snapshot),
            interfaces: interfaces(),
            master: snapshot.master.on,
            master_bpm: f64::from(snapshot.master.bpm_x100) / 100.0,
            state: state.to_owned(),
            number,
        }
    }

    /// Why the link went down, once it has: the app stops the session and
    /// shows the reason.
    pub fn down(&self) -> Option<String> {
        match self.export.as_ref()?.link_state() {
            rbl_link::LinkState::Down(why) => Some(why),
            _ => None,
        }
    }

    /// After an analysis run: the players get fresh files.
    pub fn analysis_changed(&self) {
        if let Some(export) = &self.export {
            export.analysis_changed();
        }
    }

    /// Tells a CDJ on the link to load a track from our library.
    ///
    /// The reason is carried back rather than logged: a drop that does nothing
    /// and says nothing is worse than one that says why.
    pub fn load_track(&self, player_number: u8, track_id: u32) -> Result<(), String> {
        let Some(export) = &self.export else {
            return Err("LINK is not running.".to_owned());
        };
        export
            .load_track(player_number, track_id)
            .map_err(|error| format!("The player could not be told to load that track: {error}"))
    }

    /// Become the network's tempo master, or resign.
    pub fn set_master(&self, on: bool) {
        if let Some(export) = &self.export {
            export.set_master(on);
        } else {
            tracing::debug!(on, "master asked while LINK is off; nothing to do");
        }
    }

    /// Nudge the master tempo (rekordbox's −/+ move it a whole BPM), in BPM.
    pub fn nudge_master(&self, delta_bpm: f64) {
        if let Some(export) = &self.export {
            #[allow(clippy::cast_possible_truncation)]
            let delta_x100 = (delta_bpm * 100.0).round() as i32;
            export.nudge_master(delta_x100);
        }
    }

    /// Take the current master player's tempo (rekordbox's ⟳); `false` when
    /// no player is master.
    pub fn take_master_tempo(&self) -> bool {
        self.export.as_ref().is_some_and(rbl_link::LinkExport::take_master_tempo)
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
            kind: kind_name(player.kind).to_owned(),
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
            mounted: snapshot.mounted.contains(&player.address),
        })
        .collect()
}

/// Starts the passive network watcher on the announce port, reporting peers
/// to `report`. Binding proves rekordbox is not holding the port. Returns
/// `None` when the port cannot be bound (rekordbox is running).
pub fn start_watcher<F>(report: F) -> Option<rbl_link::Watcher>
where
    F: Fn(Vec<PeerDto>) + Send + 'static,
{
    match rbl_link::Watcher::start(rbl_link::Ports::REKORDBOX.announce, move |players| {
        report(players.iter().map(PeerDto::from_player).collect());
    }) {
        Ok(watcher) => {
            tracing::info!("watching the network for players");
            Some(watcher)
        }
        Err(error) => {
            tracing::warn!(%error, "network watcher not started (rekordbox may hold the port)");
            None
        }
    }
}

/// The peers heard so far, as the window shows them.
pub fn peers(state: &AppState) -> Vec<PeerDto> {
    state.link_peers().iter().map(PeerDto::from_player).collect()
}

/// Why LINK cannot start now, if it cannot: rekordbox holds the ports.
pub fn refusal() -> Option<String> {
    if rbl_db::is_rekordbox_running() {
        return Some("rekordbox is running and holds the link ports. Quit it to turn LINK on.".to_owned());
    }
    None
}
