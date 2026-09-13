//! Our presence on the link: the packets that put `rekordbox` on a player's
//! source list, and the packets from the players that tell us who is there
//! and what they have loaded.
//!
//! Two sockets, two threads. The announce socket (UDP 50000) broadcasts our
//! keep-alive every 2.0 s and hears everyone else's. The status socket (UDP
//! 50002) broadcasts the mixer-style status every 200 ms, answers a player's
//! media query and its `46` handshake, greets a player the first time it
//! reports in, and reads every player's status packet — which is how a
//! track loaded from us is known: the player says so, naming our device
//! number as the track's source. Everything sent is what rekordbox 7.2.11
//! sent a CDJ-3000 (`rbl-prolink`'s tests hold the captured bytes).

use std::collections::HashMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use alphatheta_connect::status::types::PlayState;
use alphatheta_connect::status::utils::status_from_packet;
use alphatheta_connect::types::MediaSlot;
use parking_lot::Mutex;
use rbl_prolink::{
    connect_greeting, link_handshake_reply, packet_kind, DeviceTable, DeviceType, KeepAlive, MediaQuery,
    MediaResponse, Status, LINK_HANDSHAKE_KIND, PLAYER_STATUS_KIND, REKORDBOX_DEVICE_NUMBER, REKORDBOX_NAME,
    SLOT_REKORDBOX,
};

/// rekordbox's keep-alive interval, measured.
const KEEP_ALIVE_EVERY: Duration = Duration::from_millis(2000);
/// The startup ladder's spacing: `0a` ×3, `00` ×3, `02` ×3, `04` ×3, as
/// alphatheta-connect sends it (verified against a CDJ-3000 as a virtual
/// player). A CDJ-3000 that only ever hears keep-alives from a new device
/// does not list it; it listed rekordbox only after this. `[ASSUME]` the
/// layouts for a device of rekordbox's type — the capture began with
/// rekordbox already up.
const STARTUP_STAGE_EVERY: Duration = Duration::from_millis(300);
/// rekordbox's status interval, measured.
const STATUS_EVERY: Duration = Duration::from_millis(200);
/// How long a receive blocks before the thread looks at the clock again.
const POLL: Duration = Duration::from_millis(50);
/// A player that has not reported in for this long is no longer holding
/// anything of ours. Players send status at 5 Hz; keep-alives every 1.5 s.
const PLAYER_TIMEOUT: Duration = Duration::from_secs(6);

/// The largest packet either port carries: a CDJ-3000's status is 300
/// bytes, and the mixers' are longer still.
const DATAGRAM: usize = 2048;

/// Where the beacon runs and what it says about the library.
#[derive(Debug, Clone)]
pub struct BeaconConfig {
    pub address: Ipv4Addr,
    pub broadcast: Ipv4Addr,
    pub mac: [u8; 6],
    /// Our announce port; 0 for any free one.
    pub announce_port: u16,
    /// Our status port; 0 for any free one.
    pub status_port: u16,
    /// The port players listen on for status, replies and the greeting:
    /// 50002 on the link. A test's player binds its own.
    pub player_port: u16,
}

/// What the media response tells a player about the library. Read on every
/// query rather than fixed at start, so a reload behind us is reflected.
pub trait LibraryFacts: Send + Sync {
    fn track_count(&self) -> u16;
    fn playlist_count(&self) -> u16;
}

/// A player as its packets describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Player {
    pub number: u8,
    pub name: String,
    pub address: Ipv4Addr,
    pub kind: DeviceType,
    /// The id of the track it has loaded from us, if one.
    pub loaded: Option<u32>,
    pub playing: bool,
    pub master: bool,
    /// Tempo × 100 as the player reports it — its track's, at its pitch.
    pub bpm_x100: u32,
    pub last_seen: Instant,
}

/// Everything the threads learn, for the app to read.
#[derive(Default)]
struct Shared {
    peers: DeviceTable,
    players: HashMap<u8, Player>,
    /// Players greeted since the beacon started, by address.
    greeted: Vec<Ipv4Addr>,
    /// Players heard on the announce port and not yet greeted; the status
    /// loop sends the greeting, from the port rekordbox sends it from.
    to_greet: Vec<Ipv4Addr>,
}

/// The running beacon.
pub struct Beacon {
    stop: Arc<AtomicBool>,
    threads: Vec<std::thread::JoinHandle<()>>,
    shared: Arc<Mutex<Shared>>,
    announce_port: u16,
    status_port: u16,
}

impl Beacon {
    /// Binds both ports and starts announcing.
    pub fn start(config: BeaconConfig, facts: Arc<dyn LibraryFacts>) -> io::Result<Self> {
        // Bound to every interface, not the chosen one: on macOS a socket
        // bound to one address does not receive broadcasts. The chosen
        // interface is what we *send* on, by way of its subnet broadcast.
        //
        // Shared, unlike rekordbox's, which holds 50000 exclusively: a
        // listener beside us — the CDJ-3000 emulator's test harness hears
        // announcements on a socket of its own — costs nothing, and
        // rekordbox already running still refuses us, since its bind is
        // the exclusive one.
        let announce = shared_udp(config.announce_port)?;
        let status = shared_udp(config.status_port)?;
        for socket in [&announce, &status] {
            socket.set_broadcast(true)?;
            socket.set_read_timeout(Some(POLL))?;
        }

        // Ephemeral ports are known only now; the loops broadcast to them.
        let mut config = config;
        config.announce_port = announce.local_addr()?.port();
        config.status_port = status.local_addr()?.port();
        let (announce_port, status_port) = (config.announce_port, config.status_port);

        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Mutex::new(Shared::default()));
        let mut threads = Vec::with_capacity(2);
        {
            let (stop, shared, config) = (Arc::clone(&stop), Arc::clone(&shared), config.clone());
            threads.push(std::thread::spawn(move || announce_loop(&announce, &config, &stop, &shared)));
        }
        {
            let (stop, shared) = (Arc::clone(&stop), Arc::clone(&shared));
            threads.push(std::thread::spawn(move || status_loop(&status, &config, &stop, &shared, &facts)));
        }
        Ok(Self { stop, threads, shared, announce_port, status_port })
    }

    pub const fn announce_port(&self) -> u16 {
        self.announce_port
    }

    pub const fn status_port(&self) -> u16 {
        self.status_port
    }

    /// Every player heard from, in device-number order.
    pub fn players(&self) -> Vec<Player> {
        let mut players: Vec<Player> = self.shared.lock().players.values().cloned().collect();
        players.sort_by_key(|p| p.number);
        players
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for thread in self.threads.drain(..) {
            drop(thread.join());
        }
    }
}

impl Drop for Beacon {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// A UDP socket on every interface that other listeners may share.
fn shared_udp(port: u16) -> io::Result<UdpSocket> {
    let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None)?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.bind(&SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).into())?;
    Ok(socket.into())
}

fn now_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The startup ladder, then keep-alives every two seconds; everyone else's
/// packets into the peer table.
fn announce_loop(socket: &UdpSocket, config: &BeaconConfig, stop: &AtomicBool, shared: &Mutex<Shared>) {
    let started = Instant::now();
    let to = SocketAddr::V4(SocketAddrV4::new(config.broadcast, config.announce_port));
    let mut buffer = [0_u8; DATAGRAM];

    // rekordbox's own startup ladder, not the generic virtual-CDJ one: a
    // CDJ-3000 mounts rekordbox's library only after this exact sequence
    // (`rbl_prolink::rekordbox_startup_ladder`, measured on the wire).
    let mut ladder = rbl_prolink::rekordbox_startup_ladder(config.mac, config.address).into_iter();

    let mut next_send = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        // Send when due, but never stop reading: incoming keep-alives must be
        // heard throughout the startup ladder, not only after it.
        if Instant::now() >= next_send {
            if let Some(rung) = ladder.next() {
                next_send += STARTUP_STAGE_EVERY;
                if let Err(error) = socket.send_to(&rung, to) {
                    tracing::debug!(%error, "startup packet not sent");
                }
            } else {
                next_send += KEEP_ALIVE_EVERY;
                // Peers seen, plus ourselves.
                let peers = u8::try_from(shared.lock().peers.len() + 1).unwrap_or(u8::MAX);
                let packet = KeepAlive::rekordbox(config.mac, config.address, peers).encode();
                if let Err(error) = socket.send_to(&packet, to) {
                    tracing::debug!(%error, "keep-alive not sent");
                }
            }
        }
        match socket.recv_from(&mut buffer) {
            Ok((len, from)) => {
                let packet = buffer.get(..len).unwrap_or(&[]);
                if let Ok(keep_alive) = KeepAlive::decode(packet) {
                    if keep_alive.device_number == REKORDBOX_DEVICE_NUMBER && keep_alive.ip == config.address {
                        continue; // our own broadcast, echoed back
                    }
                    let mut shared = shared.lock();
                    let now = now_ms(started);
                    shared.peers.observe(&keep_alive, now);
                    shared.peers.expire(now);
                    // A device is listed from its keep-alive; its status
                    // fills in the rest when it comes.
                    let SocketAddr::V4(from) = from else { continue };
                    shared.players.entry(keep_alive.device_number).or_insert_with(|| Player {
                        number: keep_alive.device_number,
                        name: keep_alive.name.clone(),
                        address: *from.ip(),
                        kind: keep_alive.device_type,
                        loaded: None,
                        playing: false,
                        master: false,
                        bpm_x100: 0,
                        last_seen: Instant::now(),
                    });
                    if let Some(player) = shared.players.get_mut(&keep_alive.device_number) {
                        player.last_seen = Instant::now();
                        player.name.clone_from(&keep_alive.name);
                    }
                    // A player is greeted when first heard: in the capture the
                    // greeting is what the player's portmap query follows,
                    // six milliseconds later.
                    if keep_alive.device_type == DeviceType::Cdj
                        && !shared.greeted.contains(from.ip())
                        && !shared.to_greet.contains(from.ip())
                    {
                        shared.to_greet.push(*from.ip());
                    }
                }
            }
            Err(error) if is_timeout(&error) => {}
            Err(error) => {
                tracing::warn!(%error, "announce socket stopped");
                return;
            }
        }
        shared.lock().players.retain(|_, p| p.last_seen.elapsed() < PLAYER_TIMEOUT);
    }
}

/// Status out five times a second, players' status in, the questions a
/// player asks on this port answered.
fn status_loop(
    socket: &UdpSocket,
    config: &BeaconConfig,
    stop: &AtomicBool,
    shared: &Mutex<Shared>,
    facts: &Arc<dyn LibraryFacts>,
) {
    // Status goes where the players listen, which on the link is the same
    // port we listen on.
    let broadcast = SocketAddr::V4(SocketAddrV4::new(config.broadcast, config.player_port));
    // rekordbox sends its STATUS from an EPHEMERAL source port, not from 50002
    // (measured on the wire: 51839/59681/…, a different one each time). A CDJ
    // may key its "this is a real rekordbox source" test off that, so status
    // goes out from an ephemeral socket while `socket` stays bound to 50002 for
    // receiving and for the unicast replies (which rekordbox does send from
    // 50002). Falls back to `socket` if the extra bind fails.
    let sender = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok();
    if let Some(s) = sender.as_ref() {
        let _ = s.set_broadcast(true);
    }
    let out = sender.as_ref().unwrap_or(socket);
    let mut buffer = [0_u8; DATAGRAM];
    let mut next_send = Instant::now();
    let mut beat: u8 = 1;
    while !stop.load(Ordering::Relaxed) {
        if Instant::now() >= next_send {
            next_send += STATUS_EVERY;
            // rekordbox echoes the master player's tempo and beat. With no
            // master on the link it is `[UNKNOWN]` what rekordbox sends; the
            // capture began with a master already playing. Zero here.
            let master = shared.lock().players.values().find(|p| p.master).cloned();
            let bpm_x100 = master.as_ref().map_or(0, |p| u16::try_from(p.bpm_x100).unwrap_or(u16::MAX));
            // Idle, rekordbox sends beat 0; with a master it advances 1..4.
            let sent_beat = if bpm_x100 == 0 { 0 } else { beat };
            let packet = Status { name: REKORDBOX_NAME.to_owned(), device_number: REKORDBOX_DEVICE_NUMBER, bpm_x100, beat: sent_beat }
                .encode();
            beat = if beat >= 4 { 1 } else { beat + 1 };
            if let Err(error) = out.send_to(&packet, broadcast) {
                tracing::debug!(%error, "status not sent");
            }
        }
        let pending: Vec<Ipv4Addr> = {
            let mut shared = shared.lock();
            let pending = std::mem::take(&mut shared.to_greet);
            shared.greeted.extend(pending.iter().copied());
            pending
        };
        for player in pending {
            let greeting = connect_greeting(REKORDBOX_NAME, REKORDBOX_DEVICE_NUMBER);
            send(socket, &greeting, player, config.player_port, "greeting");
        }
        let (len, from) = match socket.recv_from(&mut buffer) {
            Ok(received) => received,
            Err(error) if is_timeout(&error) => continue,
            Err(error) => {
                tracing::warn!(%error, "status socket stopped");
                return;
            }
        };
        let packet = buffer.get(..len).unwrap_or(&[]);
        let SocketAddr::V4(from) = from else { continue };
        // Our own status comes back off the broadcast; its kind is one
        // nothing below handles.
        let Ok(kind) = packet_kind(packet) else { continue };
        match kind {
            0x05 => {
                let Ok(query) = MediaQuery::decode(packet) else { continue };
                if query.device_number != REKORDBOX_DEVICE_NUMBER || query.slot != SLOT_REKORDBOX {
                    continue;
                }
                let response = MediaResponse {
                    name: REKORDBOX_NAME.to_owned(),
                    device_number: REKORDBOX_DEVICE_NUMBER,
                    tracks: facts.track_count(),
                    playlists: facts.playlist_count(),
                }
                .encode();
                send(socket, &response, query.from, config.player_port, "media response");
            }
            LINK_HANDSHAKE_KIND => {
                let reply = link_handshake_reply(REKORDBOX_NAME, REKORDBOX_DEVICE_NUMBER);
                send(socket, &reply, *from.ip(), config.player_port, "handshake reply");
            }
            PLAYER_STATUS_KIND => {
                let Ok(Some(state)) = status_from_packet(packet) else { continue };
                let mut shared = shared.lock();
                if !shared.greeted.contains(from.ip()) {
                    // The first status from a player is what rekordbox
                    // answers with the greeting `[ASSUME]`; it sent one just
                    // before the player's portmap query.
                    shared.greeted.push(*from.ip());
                    let greeting = connect_greeting(REKORDBOX_NAME, REKORDBOX_DEVICE_NUMBER);
                    send(socket, &greeting, *from.ip(), config.player_port, "greeting");
                }
                // The player names the source device and the slot; a track
                // of ours is one it took from device 17. The slot byte says
                // `Rb` for a rekordbox source in the community analysis, but
                // the player's media query about us asks for slot 3 (USB), so
                // the slot is not relied on.
                let from_us = state.track_device_id == REKORDBOX_DEVICE_NUMBER
                    && matches!(state.track_slot, MediaSlot::Rb | MediaSlot::Usb)
                    && state.track_id != 0;
                let playing = matches!(state.play_state, PlayState::Playing | PlayState::Looping);
                let player = shared.players.entry(state.device_id).or_insert_with(|| Player {
                    number: state.device_id,
                    name: rbl_prolink::status_device_name(packet).unwrap_or_default(),
                    address: *from.ip(),
                    kind: DeviceType::Cdj,
                    loaded: None,
                    playing: false,
                    master: false,
                    bpm_x100: 0,
                    last_seen: Instant::now(),
                });
                player.loaded = from_us.then_some(state.track_id);
                player.playing = playing;
                player.master = state.is_master;
                player.bpm_x100 = tempo_x100(state.track_bpm, state.effective_pitch);
                player.last_seen = Instant::now();
            }
            _ => {}
        }
    }
}

/// The tempo a player is playing at, ×100: its track's BPM at its pitch,
/// or 0 with nothing loaded.
fn tempo_x100(track_bpm: Option<f64>, pitch_percent: f64) -> u32 {
    let Some(bpm) = track_bpm else { return 0 };
    let x100 = (bpm * 100.0 * (1.0 + pitch_percent / 100.0)).round();
    if x100.is_finite() && x100 > 0.0 && x100 < f64::from(u32::MAX) {
        // In range and rounded: the cast is exact.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            x100 as u32
        }
    } else {
        0
    }
}

fn send(socket: &UdpSocket, packet: &[u8], to: Ipv4Addr, port: u16, what: &str) {
    if let Err(error) = socket.send_to(packet, SocketAddr::V4(SocketAddrV4::new(to, port))) {
        tracing::debug!(%error, %to, what, "not sent");
    }
}

fn is_timeout(error: &io::Error) -> bool {
    matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut)
}
