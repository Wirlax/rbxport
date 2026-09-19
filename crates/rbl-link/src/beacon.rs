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
    announce_kind_name, connect_greeting, connect_identity, hex, link_handshake_reply, packet_kind,
    status_kind_name, DeviceTable, DeviceType, KeepAlive, MediaQuery, MediaResponse, Status,
    DEVICE_IDENTITY_QUERY_KIND, LINK_HANDSHAKE_KIND, LOAD_TRACK_ACK_KIND, PLAYER_STATUS_KIND,
    REKORDBOX_DEVICE_NUMBER, REKORDBOX_NAME, SLOT_REKORDBOX, SLOT_REKORDBOX_LEGACY,
};

/// How many bytes of a packet a trace line shows.
const TRACE_BYTES: usize = 64;

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
    /// The OS name of the interface `address` belongs to (`en0`,
    /// `Ethernet 2`), which the sockets are pinned to; `None` on loopback,
    /// where a test has nothing to pin to.
    pub interface: Option<String>,
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
    /// The port players listen on for beat packets: 50001 on the link. A
    /// test binds its own.
    pub beat_port: u16,
    /// This computer's name, as rekordbox puts it in the identity reply.
    pub computer_name: String,
}

/// The tempo-master state the app drives and the beat clock reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MasterState {
    /// We are the network's tempo master, broadcasting beats the others sync
    /// to.
    pub on: bool,
    /// The tempo we drive, × 100. Persists across turning master off and on.
    pub bpm_x100: u16,
    /// The beat within the bar, 1 to 4; the beat clock advances it.
    pub bar_beat: u8,
}

impl Default for MasterState {
    fn default() -> Self {
        // 120.00 BPM until the DJ nudges it or takes a player's tempo, as a
        // resting default; rekordbox shows the last value it held.
        Self { on: false, bpm_x100: 12_000, bar_beat: 1 }
    }
}

/// The slowest and fastest master tempo the nudge will reach, × 100
/// (40.00 to 300.00 BPM), so a runaway nudge cannot send a meaningless
/// tempo onto the link.
const MASTER_BPM_MIN: u16 = 4_000;
const MASTER_BPM_MAX: u16 = 30_000;

/// What the media response tells a player about the library, and what the
/// players' status tells the library. Read on every query rather than fixed
/// at start, so a reload behind us is reflected.
pub trait LibraryFacts: Send + Sync {
    fn track_count(&self) -> u16;
    fn playlist_count(&self) -> u16;
    /// A player has just loaded one of our tracks: once per load, not per
    /// status packet.
    fn track_loaded(&self, _track: u32) {}
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
    /// Our tempo-master state; the beat clock and the status loop share it.
    master: MasterState,
}

/// The running beacon.
pub struct Beacon {
    stop: Arc<AtomicBool>,
    threads: Vec<std::thread::JoinHandle<()>>,
    shared: Arc<Mutex<Shared>>,
    announce_port: u16,
    status_port: u16,
    /// The status socket again, for commands sent from outside its loop.
    /// A command must leave from the port the player has us at, not from a
    /// fresh ephemeral one — that is the source it answers to.
    commands: UdpSocket,
    /// Where the players listen: 50002 on the link, a test's own port.
    player_port: u16,
}

impl Beacon {
    /// Binds both ports and starts announcing.
    pub fn start(config: BeaconConfig, facts: Arc<dyn LibraryFacts>) -> io::Result<Self> {
        // Pinned to the chosen interface, so everything leaves from the
        // address the keep-alive announces. A player answers a command only
        // from that address: with two interfaces on the players' subnet the
        // OS otherwise routes our unicast out whichever it likes, and a
        // CDJ-3000 told to load a track from the other one does nothing.
        //
        // Shared, unlike rekordbox's, which holds 50000 exclusively: a
        // listener beside us — the CDJ-3000 emulator's test harness hears
        // announcements on a socket of its own — costs nothing, and
        // rekordbox already running still refuses us, since its bind is
        // the exclusive one.
        let pin = config.interface.as_deref().map(|name| (name, config.address));
        let announce = shared_udp(config.announce_port, pin)?;
        let status = shared_udp(config.status_port, pin)?;
        for socket in [&announce, &status] {
            socket.set_broadcast(true)?;
            socket.set_read_timeout(Some(POLL))?;
        }

        // Ephemeral ports are known only now; the loops broadcast to them.
        let mut config = config;
        config.announce_port = announce.local_addr()?.port();
        config.status_port = status.local_addr()?.port();
        let (announce_port, status_port) = (config.announce_port, config.status_port);
        tracing::debug!(
            interface = config.interface.as_deref().unwrap_or("any"),
            address = %config.address,
            broadcast = %config.broadcast,
            mac = %hex(&config.mac, 6),
            announce_port,
            status_port,
            player_port = config.player_port,
            beat_port = config.beat_port,
            computer_name = %config.computer_name,
            "beacon bound"
        );

        // Kept before the loop takes ownership: a load command has to go out
        // from this same port, so the player sees it from the device it knows.
        let commands = status.try_clone()?;
        let player_port = config.player_port;

        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Mutex::new(Shared::default()));
        let mut threads = Vec::with_capacity(3);
        {
            let (stop, shared, config) = (Arc::clone(&stop), Arc::clone(&shared), config.clone());
            threads.push(std::thread::spawn(move || announce_loop(&announce, &config, &stop, &shared)));
        }
        {
            let (stop, shared) = (Arc::clone(&stop), Arc::clone(&shared));
            let config = config.clone();
            threads.push(std::thread::spawn(move || status_loop(&status, &config, &stop, &shared, &facts)));
        }
        // The beat clock broadcasts a beat on its own socket, tempo-locked,
        // only while we are master; a bind failure loses only the beats, not
        // the rest of LINK, so it falls back to nothing rather than aborting.
        if let Ok(beats) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).and_then(|s| {
            s.set_broadcast(true)?;
            Ok(s)
        }) {
            let (stop, shared, config) = (Arc::clone(&stop), Arc::clone(&shared), config.clone());
            threads.push(std::thread::spawn(move || beat_clock(&beats, &config, &stop, &shared)));
        } else {
            tracing::warn!("beat clock socket could not bind; LINK master will not drive tempo");
        }
        Ok(Self { stop, threads, shared, announce_port, status_port, commands, player_port })
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

    /// Tells player `player_number` to load `track_id` from our library.
    /// Returns an error when the player is unknown or the packet cannot be sent.
    pub fn load_track(&self, player_number: u8, track_id: u32) -> io::Result<()> {
        let address = self.shared.lock().players.get(&player_number).map(|p| p.address);
        let Some(address) = address else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("player {player_number} is not on the link"),
            ));
        };
        let packet = rbl_prolink::load_track_command(
            REKORDBOX_NAME,
            REKORDBOX_DEVICE_NUMBER,
            player_number,
            track_id,
        );
        let to = SocketAddr::V4(SocketAddrV4::new(address, self.player_port));
        let sent = self.commands.send_to(&packet, to)?;
        tracing::info!(player_number, track_id, %address, sent, "load track sent");
        tracing::trace!(%to, bytes = %hex(&packet, packet.len()), "load track packet");
        Ok(())
    }

    /// Our tempo-master state, for the app to show.
    pub fn master_state(&self) -> MasterState {
        self.shared.lock().master
    }

    /// Become the network's tempo master, or resign. Becoming master keeps
    /// whatever BPM is set; the beat clock starts driving beats at once and
    /// the status packets say we are master.
    ///
    /// Asserting master is enough — no handoff protocol is needed. Verified
    /// live (2026-09-14) on a real CDJ-3000: a deck that was itself master
    /// yields the moment it sees our master status and follows our tempo,
    /// sending no `0x26`/`0x27` handoff of its own, and when we resign a
    /// synced, playing deck takes master over on its own.
    pub fn set_master(&self, on: bool) {
        let mut shared = self.shared.lock();
        shared.master.on = on;
        if on {
            // Start each master run on the downbeat.
            shared.master.bar_beat = 1;
        }
        tracing::info!(on, bpm_x100 = shared.master.bpm_x100, "link master");
    }

    /// Set the master tempo (× 100), clamped to a sane range. Used by the
    /// "take the current master's tempo" button and any direct set.
    pub fn set_master_bpm(&self, bpm_x100: u16) {
        let mut shared = self.shared.lock();
        shared.master.bpm_x100 = bpm_x100.clamp(MASTER_BPM_MIN, MASTER_BPM_MAX);
        tracing::debug!(asked = bpm_x100, set = shared.master.bpm_x100, "master tempo set");
    }

    /// Nudge the master tempo by `delta_x100` (rekordbox's −/+ move it a whole
    /// BPM), clamped to the same range.
    pub fn nudge_master(&self, delta_x100: i32) {
        let mut shared = self.shared.lock();
        let next = i32::from(shared.master.bpm_x100) + delta_x100;
        let clamped = next.clamp(i32::from(MASTER_BPM_MIN), i32::from(MASTER_BPM_MAX));
        shared.master.bpm_x100 = u16::try_from(clamped).unwrap_or(MASTER_BPM_MIN);
        tracing::debug!(delta_x100, bpm_x100 = shared.master.bpm_x100, "master tempo nudged");
    }

    /// The tempo a player on the link currently reports as master, × 100, or
    /// `None` when no player is master. What the "take the master's tempo"
    /// button reads.
    pub fn current_player_tempo(&self) -> Option<u16> {
        self.shared
            .lock()
            .players
            .values()
            .find(|p| p.master && p.bpm_x100 != 0)
            .map(|p| u16::try_from(p.bpm_x100).unwrap_or(u16::MAX))
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for thread in self.threads.drain(..) {
            drop(thread.join());
        }
        tracing::debug!("beacon stopped");
    }
}

impl Drop for Beacon {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// A UDP socket other listeners may share, pinned to one interface when
/// `pin` names it (with the address on it), or on every interface.
///
/// Pinning is by interface rather than by binding to the address, because
/// on macOS and Linux a socket bound to one address hears no broadcasts,
/// and the keep-alives are broadcasts. Windows delivers broadcasts to such
/// a socket, and has no interface pin for them, so there the address is
/// bound.
fn shared_udp(port: u16, pin: Option<(&str, Ipv4Addr)>) -> io::Result<UdpSocket> {
    let socket = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None)?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    let bind_to = match pin {
        Some((_, address)) if cfg!(windows) => address,
        _ => Ipv4Addr::UNSPECIFIED,
    };
    socket.bind(&SocketAddr::V4(SocketAddrV4::new(bind_to, port)).into())?;
    if let Some((name, _)) = pin {
        pin_to_interface(&socket, name)?;
    }
    Ok(socket.into())
}

/// `IP_BOUND_IF`: sends leave by this interface, from its address, and only
/// what arrives on it is received. It takes the interface's index, which
/// the OS lists beside the name.
#[cfg(target_vendor = "apple")]
fn pin_to_interface(socket: &socket2::Socket, name: &str) -> io::Result<()> {
    let index = if_addrs::get_if_addrs()?
        .into_iter()
        .find(|i| i.name == name)
        .and_then(|i| i.index)
        .and_then(std::num::NonZeroU32::new)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no network interface called {name}")))?;
    socket.bind_device_by_index_v4(Some(index))
}

/// `SO_BINDTODEVICE`, the same by name.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn pin_to_interface(socket: &socket2::Socket, name: &str) -> io::Result<()> {
    socket.bind_device(Some(name.as_bytes()))
}

/// Bound to the interface's address instead, above.
#[cfg(not(any(target_vendor = "apple", target_os = "linux", target_os = "android")))]
fn pin_to_interface(_socket: &socket2::Socket, _name: &str) -> io::Result<()> {
    Ok(())
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
                match socket.send_to(&rung, to) {
                    Ok(_) => tracing::debug!(
                        kind = %rung.get(0x0a).map_or_else(|| "?".to_owned(), |k| announce_kind_name(*k)),
                        len = rung.len(),
                        %to,
                        "startup rung sent"
                    ),
                    Err(error) => tracing::warn!(%error, "startup packet not sent"),
                }
            } else {
                next_send += KEEP_ALIVE_EVERY;
                // The count of *other* devices we see, not counting ourselves:
                // captured 2026-09-12 against rekordbox 7.2.11, which sent 0x02
                // at keep-alive offset 0x30 with a deck and one other client on
                // the LAN (two peers), where rbxport had been sending 0x03.
                let peers = u8::try_from(shared.lock().peers.len()).unwrap_or(u8::MAX);
                let packet = KeepAlive::rekordbox(config.mac, config.address, peers).encode();
                match socket.send_to(&packet, to) {
                    Ok(_) => tracing::trace!(peers, %to, "keep-alive sent"),
                    Err(error) => tracing::warn!(%error, "keep-alive not sent"),
                }
            }
        }
        match socket.recv_from(&mut buffer) {
            Ok((len, from)) => hear_announce(buffer.get(..len).unwrap_or(&[]), from, config, shared, started),
            Err(error) if is_timeout(&error) => {}
            Err(error) => {
                tracing::error!(%error, "announce socket stopped; the link will not hear new devices");
                return;
            }
        }
        shared.lock().players.retain(|number, p| {
            let alive = p.last_seen.elapsed() < PLAYER_TIMEOUT;
            if !alive {
                tracing::info!(number, name = %p.name, address = %p.address, "device silent for 6 s; gone from the link");
            }
            alive
        });
    }
    tracing::debug!("announce loop stopped");
}

/// A packet off the announce port: a keep-alive into the peer table and the
/// player list, a first-heard player queued for the greeting.
fn hear_announce(packet: &[u8], from: SocketAddr, config: &BeaconConfig, shared: &Mutex<Shared>, started: Instant) {
    let len = packet.len();
    tracing::trace!(
        %from,
        len,
        kind = %packet_kind(packet).map_or_else(|_| "not a link packet".to_owned(), announce_kind_name),
        bytes = %hex(packet, TRACE_BYTES),
        "announce port received"
    );
    if let Ok(keep_alive) = KeepAlive::decode(packet) {
        if keep_alive.device_number == REKORDBOX_DEVICE_NUMBER && keep_alive.ip == config.address {
            return; // our own broadcast, echoed back
        }
        let mut shared = shared.lock();
        let now = now_ms(started);
        let known = shared.peers.peers().iter().any(|p| p.device_number == keep_alive.device_number);
        shared.peers.observe(&keep_alive, now);
        let expired = shared.peers.expire(now);
        if expired > 0 {
            tracing::debug!(expired, "peers timed out of the keep-alive table");
        }
        if !known {
            tracing::info!(
                number = keep_alive.device_number,
                name = %keep_alive.name,
                kind = ?keep_alive.device_type,
                ip = %keep_alive.ip,
                mac = %hex(&keep_alive.mac, 6),
                "new device on the link"
            );
        }
        // A device is listed from its keep-alive; its status
        // fills in the rest when it comes.
        let SocketAddr::V4(from) = from else { return };
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
            tracing::debug!(player = %from.ip(), "player heard for the first time; greeting queued");
            shared.to_greet.push(*from.ip());
        }
    } else {
        tracing::trace!(%from, len, "announce port packet is not a keep-alive");
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
    tracing::debug!(
        status_from = %out.local_addr().map_or_else(|e| e.to_string(), |a| a.to_string()),
        replies_from = %socket.local_addr().map_or_else(|e| e.to_string(), |a| a.to_string()),
        %broadcast,
        "status loop started"
    );
    let mut buffer = [0_u8; DATAGRAM];
    let mut next_send = Instant::now();
    let mut beat: u8 = 1;
    while !stop.load(Ordering::Relaxed) {
        if Instant::now() >= next_send {
            next_send += STATUS_EVERY;
            let packet = status_packet(&shared.lock(), beat);
            beat = if beat >= 4 { 1 } else { beat + 1 };
            match out.send_to(&packet, broadcast) {
                Ok(_) => tracing::trace!(len = packet.len(), bytes = %hex(&packet, TRACE_BYTES), "status sent"),
                Err(error) => tracing::warn!(%error, "status not sent"),
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
                tracing::error!(%error, "status socket stopped; players will not be answered");
                return;
            }
        };
        let packet = buffer.get(..len).unwrap_or(&[]);
        let SocketAddr::V4(from) = from else { continue };
        // Our own status comes back off the broadcast; its kind is one
        // nothing below handles.
        let Ok(kind) = packet_kind(packet) else {
            tracing::trace!(%from, len, bytes = %hex(packet, TRACE_BYTES), "status port received a non-link packet");
            continue;
        };
        if from.ip() != &config.address {
            tracing::trace!(
                %from,
                len,
                kind = %status_kind_name(kind),
                bytes = %hex(packet, TRACE_BYTES),
                "status port received"
            );
        }
        match kind {
            DEVICE_IDENTITY_QUERY_KIND => {
                // The player announces itself with `10`; rekordbox answers
                // with its own identity (`11`), and only then does the player
                // go on to the media query and the mount. Sent to the status
                // port, as rekordbox sends it, not the player's source port.
                tracing::debug!(player = %from.ip(), "device identity query; answering with ours");
                let identity = connect_identity(REKORDBOX_NAME, REKORDBOX_DEVICE_NUMBER, &config.computer_name);
                send(socket, &identity, *from.ip(), config.player_port, "identity");
            }
            0x05 => answer_media_query(socket, packet, config, facts.as_ref()),
            LINK_HANDSHAKE_KIND => {
                tracing::debug!(player = %from.ip(), "link handshake; answering");
                let reply = link_handshake_reply(REKORDBOX_NAME, REKORDBOX_DEVICE_NUMBER);
                send(socket, &reply, *from.ip(), config.player_port, "handshake reply");
            }
            // A player that accepts a load command says so with `1a`; the
            // load itself shows up in its next status packets.
            LOAD_TRACK_ACK_KIND => {
                tracing::info!(from = %from.ip(), "player accepted a load track command");
            }
            PLAYER_STATUS_KIND => hear_player_status(packet, from, len, socket, config, shared, facts),
            _ => {
                if from.ip() != &config.address {
                    tracing::trace!(%from, kind = %status_kind_name(kind), "status port packet not handled");
                }
            }
        }
    }
    tracing::debug!("status loop stopped");
}

/// A player's status packet: what it has loaded, whether it plays, whether
/// it is master; the greeting on its first one.
fn hear_player_status(
    packet: &[u8],
    from: SocketAddrV4,
    len: usize,
    socket: &UdpSocket,
    config: &BeaconConfig,
    shared: &Mutex<Shared>,
    facts: &Arc<dyn LibraryFacts>,
) {
        let state = match status_from_packet(packet) {
            Ok(Some(state)) => state,
            Ok(None) => {
                tracing::trace!(%from, len, "status packet not a player's; ignored");
                return;
            }
            Err(error) => {
                tracing::warn!(%from, len, %error, "status packet could not be read");
                return;
            }
        };
        tracing::trace!(
            %from,
            device = state.device_id,
            track_id = state.track_id,
            track_device = state.track_device_id,
            track_slot = ?state.track_slot,
            play_state = ?state.play_state,
            master = state.is_master,
            bpm = ?state.track_bpm,
            pitch = state.effective_pitch,
            "player status"
        );
        let mut shared = shared.lock();
        if !shared.greeted.contains(from.ip()) {
            // The first status from a player is what rekordbox
            // answers with the greeting `[ASSUME]`; it sent one just
            // before the player's portmap query.
            tracing::debug!(player = %from.ip(), "first status from a player; greeting it");
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
        // The status packet does not name the device kind; the
        // keep-alive does. Read it from the peer table by number so a
        // mixer is shown as a mixer, not a player.
        let kind = peer_kind(&shared, state.device_id);
        let player = shared.players.entry(state.device_id).or_insert_with(|| {
            let name = rbl_prolink::status_device_name(packet).unwrap_or_default();
            tracing::debug!(number = state.device_id, %name, address = %from.ip(), "player listed from its status");
            Player {
                number: state.device_id,
                name,
                address: *from.ip(),
                kind,
                loaded: None,
                playing: false,
                master: false,
                bpm_x100: 0,
                last_seen: Instant::now(),
            }
        });
        let loaded = from_us.then_some(state.track_id);
        if player.loaded != loaded {
            tracing::info!(
                number = player.number,
                was = player.loaded,
                now = loaded,
                track_device = state.track_device_id,
                track_slot = ?state.track_slot,
                "player's loaded track of ours changed"
            );
            if let Some(track) = loaded {
                facts.track_loaded(track);
            }
        }
        if player.playing != playing {
            tracing::debug!(number = player.number, playing, play_state = ?state.play_state, "player play state changed");
        }
        if player.master != state.is_master {
            tracing::info!(number = player.number, master = state.is_master, "player master state changed");
        }
        player.kind = kind;
        player.loaded = loaded;
        player.playing = playing;
        player.master = state.is_master;
        player.bpm_x100 = tempo_x100(state.track_bpm, state.effective_pitch);
        player.last_seen = Instant::now();
}

/// The status packet to broadcast now. When we are master we say so, at our
/// own tempo and the beat clock's beat. Otherwise we echo the master player's
/// tempo with a beat that free-runs at the status rate (rekordbox echoes the
/// master's; with no master on the link it is `[UNKNOWN]`, so zero). The
/// status flag and Mm say which of the two this is.
fn status_packet(shared: &Shared, free_beat: u8) -> Vec<u8> {
    let master = shared.master;
    let (bpm_x100, beat, we_master) = if master.on {
        (master.bpm_x100, master.bar_beat, true)
    } else {
        let mirror = shared
            .players
            .values()
            .find(|p| p.master)
            .map_or(0, |p| u16::try_from(p.bpm_x100).unwrap_or(u16::MAX));
        (mirror, if mirror == 0 { 0 } else { free_beat }, false)
    };
    Status { name: REKORDBOX_NAME.to_owned(), device_number: REKORDBOX_DEVICE_NUMBER, bpm_x100, beat, master: we_master }
        .encode()
}

/// Broadcasts a beat packet on each beat while we are master, and advances
/// the shared bar beat 1 → 2 → 3 → 4. Idle, it waits.
///
/// The next beat is scheduled from the last rather than from a fresh sleep,
/// so the tempo does not drift with the OS's sleep granularity; a nudge is
/// picked up on the next beat because the interval is read each time.
fn beat_clock(socket: &UdpSocket, config: &BeaconConfig, stop: &AtomicBool, shared: &Mutex<Shared>) {
    let to = SocketAddr::V4(SocketAddrV4::new(config.broadcast, config.beat_port));
    let mut next_beat = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let (on, bpm_x100, bar_beat) = {
            let s = shared.lock();
            (s.master.on, s.master.bpm_x100, s.master.bar_beat)
        };
        let now = Instant::now();
        if !on {
            // The first beat on becoming master falls at once.
            next_beat = now;
            std::thread::sleep(POLL);
            continue;
        }
        if now < next_beat {
            std::thread::sleep((next_beat - now).min(POLL));
            continue;
        }
        let packet = rbl_prolink::beat_packet(REKORDBOX_NAME, REKORDBOX_DEVICE_NUMBER, bpm_x100, bar_beat);
        match socket.send_to(&packet, to) {
            Ok(_) => tracing::trace!(bpm_x100, bar_beat, %to, "beat sent"),
            Err(error) => tracing::warn!(%error, "beat not sent"),
        }
        shared.lock().master.bar_beat = if bar_beat >= 4 { 1 } else { bar_beat + 1 };
        // 60000/bpm ms a beat; bpm is × 100, so 6_000_000 / bpm_x100 ms.
        let interval = Duration::from_millis(6_000_000 / u64::from(bpm_x100.max(1)));
        next_beat += interval;
        // Behind by more than a beat (a tempo jump, or the thread was
        // starved): resync rather than fire a burst to catch up.
        if next_beat < now {
            next_beat = now + interval;
        }
    }
}

/// Answers a player asking what is in our rekordbox slot with the library's
/// counts, naming back whichever slot number it used for us — `04` from a
/// current CDJ-3000, `03` from the EP122 emulator — as rekordbox does. A
/// question about any other device or slot is not ours to answer.
fn answer_media_query(socket: &UdpSocket, packet: &[u8], config: &BeaconConfig, facts: &dyn LibraryFacts) {
    let query = match MediaQuery::decode(packet) {
        Ok(query) => query,
        Err(error) => {
            tracing::warn!(%error, len = packet.len(), "media query could not be read");
            return;
        }
    };
    if query.device_number != REKORDBOX_DEVICE_NUMBER || ![SLOT_REKORDBOX, SLOT_REKORDBOX_LEGACY].contains(&query.slot) {
        tracing::trace!(
            from = %query.from,
            device = query.device_number,
            slot = query.slot,
            "media query about another device or slot; not ours to answer"
        );
        return;
    }
    let (tracks, playlists) = (facts.track_count(), facts.playlist_count());
    tracing::debug!(from = %query.from, slot = query.slot, tracks, playlists, "media query about our slot; answering");
    let response = MediaResponse {
        name: REKORDBOX_NAME.to_owned(),
        device_number: REKORDBOX_DEVICE_NUMBER,
        slot: query.slot,
        tracks,
        playlists,
    }
    .encode();
    send(socket, &response, query.from, config.player_port, "media response");
}

/// The device kind of the peer with `number`, from the keep-alive table, or a
/// player until its keep-alive has been heard.
fn peer_kind(shared: &Shared, number: u8) -> DeviceType {
    shared.peers.peers().iter().find(|p| p.device_number == number).map_or(DeviceType::Cdj, |p| p.device_type)
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
    match socket.send_to(packet, SocketAddr::V4(SocketAddrV4::new(to, port))) {
        Ok(_) => tracing::debug!(what, %to, port, len = packet.len(), "sent"),
        Err(error) => tracing::warn!(%error, %to, what, "not sent"),
    }
}

fn is_timeout(error: &io::Error) -> bool {
    matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut)
}
