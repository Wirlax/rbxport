//! Pro DJ Link packets.
//!
//! Every packet on the DJ Link network begins with the same ten-byte magic,
//! a one-byte kind, a subtype, and a twenty-byte device name. What follows
//! depends on the kind and the port it arrived on.
//!
//! # What is documented and what is not
//!
//! Layouts here follow the community protocol analysis (Deep Symmetry's
//! `djl-analysis`), which was derived from captures of real hardware. Values
//! specific to *rekordbox announcing itself as a media source* — device number
//! `0x11`, device type `0x04` — are recorded there but have not been verified
//! against a capture on this machine, so they are marked and must be confirmed
//! before anyone relies on a player accepting them.
//!
//! Nothing in this crate opens a socket; it is encoding and decoding only, so
//! it can be tested exhaustively without a network.

use std::net::Ipv4Addr;

/// Every DJ Link packet starts with this.
pub const MAGIC: [u8; 10] = [0x51, 0x73, 0x70, 0x74, 0x31, 0x57, 0x6d, 0x4a, 0x4f, 0x4c];

/// Device names are a fixed twenty bytes, NUL-padded.
pub const NAME_LEN: usize = 20;
/// Offset of the device name.
pub const NAME_AT: usize = 0x0c;

/// Announcement and keep-alive.
pub const PORT_ANNOUNCE: u16 = 50_000;
/// Beats and mixer features.
pub const PORT_BEAT: u16 = 50_001;
/// Player status.
pub const PORT_STATUS: u16 = 50_002;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PacketError {
    #[error("packet is too short: {0} bytes")]
    TooShort(usize),
    #[error("not a DJ Link packet: wrong magic")]
    BadMagic,
    #[error("unexpected packet kind {0:#04x} for this port")]
    WrongKind(u8),
}

pub type Result<T> = std::result::Result<T, PacketError>;

/// Packet kinds seen on port 50000.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnounceKind {
    /// First-stage device number claim.
    ClaimStage1,
    /// Second-stage claim.
    ClaimStage2,
    /// Final-stage claim.
    ClaimFinal,
    /// Still present on the network.
    KeepAlive,
    /// Another device is claiming the same number.
    Conflict,
    /// Initial "I am here".
    Announce,
    Other(u8),
}

impl AnnounceKind {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0x00 => Self::ClaimStage1,
            0x02 => Self::ClaimStage2,
            0x04 => Self::ClaimFinal,
            0x06 => Self::KeepAlive,
            0x08 => Self::Conflict,
            0x0a => Self::Announce,
            other => Self::Other(other),
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            Self::ClaimStage1 => 0x00,
            Self::ClaimStage2 => 0x02,
            Self::ClaimFinal => 0x04,
            Self::KeepAlive => 0x06,
            Self::Conflict => 0x08,
            Self::Announce => 0x0a,
            Self::Other(v) => v,
        }
    }
}

/// What kind of device is speaking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    Cdj,
    Mixer,
    /// rekordbox acting as a media source (measured: byte `0x34` = `04`).
    Rekordbox,
    Other(u8),
}

impl DeviceType {
    pub fn to_u8(self) -> u8 {
        match self {
            Self::Cdj => 0x01,
            Self::Mixer => 0x02,
            Self::Rekordbox => 0x04,
            Self::Other(v) => v,
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            0x01 => Self::Cdj,
            0x02 => Self::Mixer,
            0x04 => Self::Rekordbox,
            other => Self::Other(other),
        }
    }
}

/// The device number rekordbox takes when it announces itself (measured;
/// the CDJ-3000 lists it as `USB LINK17`).
pub const REKORDBOX_DEVICE_NUMBER: u8 = 0x11;

/// The name rekordbox announces.
pub const REKORDBOX_NAME: &str = "rekordbox";

/// How often a keep-alive goes out: rekordbox 7.2.11 sends one every 2.0 s
/// and a CDJ-3000 every 1.5 s (measured). A peer that has not been heard
/// from in three intervals is considered gone.
pub const KEEP_ALIVE_INTERVAL_MS: u64 = 2_000;

/// A parsed keep-alive (`kind 06`), the packet that says a device is present.
///
/// Layout, measured from rekordbox 7.2.11, a CDJ-3000 and Now Playing on the
/// same network (`docs/pre-release/design-notes/link-export-capture.md`):
/// after the name, `01`, a generation byte (`03` for rekordbox 7 and the
/// CDJ-3000, `02` for older players and virtual CDJs), the length `0036`,
/// the device number, `01`, the MAC, the IP, the peer count, a byte that is
/// `01` on rekordbox and `00` on a CDJ-3000, two zeros, the device type
/// (`04` rekordbox, `01` player, `02` mixer), and a final byte (`08` on
/// rekordbox, `64` on a CDJ-3000).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeepAlive {
    pub name: String,
    pub device_number: u8,
    pub device_type: DeviceType,
    pub mac: [u8; 6],
    pub ip: Ipv4Addr,
    /// Devices seen on the network, including this one.
    pub peers: u8,
    /// Byte `0x21`: `03` for rekordbox 7 and CDJ-3000 class devices, `02`
    /// for the rest.
    pub generation: u8,
}

/// Byte length of a keep-alive.
pub const KEEP_ALIVE_LEN: usize = 0x36;

fn write_header(out: &mut Vec<u8>, kind: u8, subtype: u8, name: &str) {
    out.extend_from_slice(&MAGIC);
    out.push(kind);
    out.push(subtype);
    let mut padded = [0_u8; NAME_LEN];
    for (slot, byte) in padded.iter_mut().zip(name.as_bytes()) {
        *slot = *byte;
    }
    out.extend_from_slice(&padded);
}

/// Reads the device name from any DJ Link packet.
pub fn device_name(packet: &[u8]) -> Result<String> {
    if packet.len() < NAME_AT + NAME_LEN {
        return Err(PacketError::TooShort(packet.len()));
    }
    if packet.get(0..10) != Some(&MAGIC) {
        return Err(PacketError::BadMagic);
    }
    let raw = packet.get(NAME_AT..NAME_AT + NAME_LEN).unwrap_or(&[]);
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    Ok(String::from_utf8_lossy(raw.get(..end).unwrap_or(&[])).into_owned())
}

/// The device name of a status-port packet (50002), where the name follows
/// the kind byte directly, at `0x0b`.
pub fn status_device_name(packet: &[u8]) -> Result<String> {
    if packet.len() < NAME_AT - 1 + NAME_LEN {
        return Err(PacketError::TooShort(packet.len()));
    }
    if packet.get(0..10) != Some(&MAGIC) {
        return Err(PacketError::BadMagic);
    }
    let raw = packet.get(NAME_AT - 1..NAME_AT - 1 + NAME_LEN).unwrap_or(&[]);
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    Ok(String::from_utf8_lossy(raw.get(..end).unwrap_or(&[])).into_owned())
}

/// The kind byte of any DJ Link packet.
pub fn packet_kind(packet: &[u8]) -> Result<u8> {
    if packet.len() <= 0x0a {
        return Err(PacketError::TooShort(packet.len()));
    }
    if packet.get(0..10) != Some(&MAGIC) {
        return Err(PacketError::BadMagic);
    }
    Ok(packet.get(0x0a).copied().unwrap_or(0))
}

impl KeepAlive {
    /// rekordbox's own keep-alive for a given address: device 17, type 4,
    /// generation 3, and the tail bytes rekordbox 7.2.11 sends.
    pub fn rekordbox(mac: [u8; 6], ip: Ipv4Addr, peers: u8) -> Self {
        Self {
            name: REKORDBOX_NAME.to_owned(),
            device_number: REKORDBOX_DEVICE_NUMBER,
            device_type: DeviceType::Rekordbox,
            mac,
            ip,
            peers,
            generation: 0x03,
        }
    }

    /// Encodes the packet.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(KEEP_ALIVE_LEN);
        write_header(&mut out, AnnounceKind::KeepAlive.to_u8(), 0x00, &self.name);
        out.push(0x01);
        out.push(self.generation);
        out.extend_from_slice(&u16::try_from(KEEP_ALIVE_LEN).unwrap_or(0).to_be_bytes());
        out.push(self.device_number);
        out.push(0x01);
        out.extend_from_slice(&self.mac);
        out.extend_from_slice(&self.ip.octets());
        out.push(self.peers);
        let (after_peers, last) = match self.device_type {
            DeviceType::Rekordbox => (0x01, 0x08),
            _ => (0x00, 0x64),
        };
        out.extend_from_slice(&[after_peers, 0, 0]);
        out.push(self.device_type.to_u8());
        out.push(last);
        debug_assert_eq!(out.len(), KEEP_ALIVE_LEN);
        out
    }

    /// Decodes a keep-alive.
    pub fn decode(packet: &[u8]) -> Result<Self> {
        if packet.len() < KEEP_ALIVE_LEN {
            return Err(PacketError::TooShort(packet.len()));
        }
        let kind = packet_kind(packet)?;
        if AnnounceKind::from_u8(kind) != AnnounceKind::KeepAlive {
            return Err(PacketError::WrongKind(kind));
        }
        let at = |i: usize| packet.get(i).copied().unwrap_or(0);
        let mut mac = [0_u8; 6];
        for (slot, byte) in mac.iter_mut().zip(packet.get(0x26..0x2c).unwrap_or(&[])) {
            *slot = *byte;
        }
        Ok(Self {
            name: device_name(packet)?,
            generation: at(0x21),
            device_number: at(0x24),
            device_type: DeviceType::from_u8(at(0x34)),
            mac,
            ip: Ipv4Addr::new(at(0x2c), at(0x2d), at(0x2e), at(0x2f)),
            peers: at(0x30),
        })
    }
}

/// The status rekordbox broadcasts on port 50002 five times a second: the
/// mixer-style packet (`kind 29`), 56 bytes, carrying the master tempo and
/// a beat counter (measured from rekordbox 7.2.11; players show the tempo
/// as MASTER BPM).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub name: String,
    pub device_number: u8,
    pub bpm_x100: u16,
    /// 1 to 4, advancing with the beat; what the value means when nothing
    /// plays is `[UNKNOWN]` — rekordbox kept sending 1.
    pub beat: u8,
}

/// Byte length of a status packet.
pub const STATUS_LEN: usize = 0x38;

/// Status-port packets (50002) carry the name straight after the kind byte,
/// with no subtype: name at `0x0b`, then the fields.
fn write_status_header(out: &mut Vec<u8>, kind: u8, name: &str) {
    out.extend_from_slice(&MAGIC);
    out.push(kind);
    let mut padded = [0_u8; NAME_LEN];
    for (slot, byte) in padded.iter_mut().zip(name.as_bytes()) {
        *slot = *byte;
    }
    out.extend_from_slice(&padded);
}

impl Status {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(STATUS_LEN);
        write_status_header(&mut out, 0x29, &self.name);
        out.extend_from_slice(&[0x01, 0x01, self.device_number, 0x00, 0x38, self.device_number]);
        out.extend_from_slice(&[0x00, 0x00, 0xc0, 0x00, 0x10, 0x00, 0x00, 0x80, 0x00]);
        out.extend_from_slice(&self.bpm_x100.to_be_bytes());
        out.extend_from_slice(&[0x00, 0x10, 0x00, 0x00, 0x00, 0x09, 0xff, self.beat]);
        debug_assert_eq!(out.len(), STATUS_LEN);
        out
    }
}

/// Byte length of the connect greeting.
pub const CONNECT_GREETING_LEN: usize = 0x30;

/// The packet rekordbox unicasts to a player's port 50002 when the player
/// connects (`kind 16`, 48 bytes; measured, meaning unknown).
pub fn connect_greeting(name: &str, device_number: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(CONNECT_GREETING_LEN);
    write_status_header(&mut out, 0x16, name);
    out.extend_from_slice(&[0x01, 0x01, device_number]);
    out.resize(CONNECT_GREETING_LEN, 0);
    out
}

/// The startup ladder rekordbox 7.2.11 broadcasts before it settles into
/// keep-alives, measured on the wire (2026-09-12): three first-stage claims
/// (`00`), then a second-stage claim (`02`) for each of the six device
/// numbers it reserves — 0x11, 0x12, 0x29, 0x2a, 0x2b, 0x2c — repeated with
/// a counter 1..=6. A CDJ-3000 mounts rekordbox's library only after this
/// sequence, not from the generic virtual-CDJ ladder. `[OBS]` the numbers
/// and the shape; why rekordbox reserves that particular block is `[UNKNOWN]`.
pub const REKORDBOX_CLAIM_NUMBERS: [u8; 6] = [0x11, 0x12, 0x29, 0x2a, 0x2b, 0x2c];

/// One first-stage claim (`00`, 44 bytes): the counter, `04`, then the MAC.
pub fn rekordbox_claim_stage1(mac: [u8; 6], counter: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(0x2c);
    write_header(&mut out, 0x00, 0x00, REKORDBOX_NAME);
    out.extend_from_slice(&[0x01, 0x03, 0x00, 0x2c]);
    out.push(counter);
    out.push(0x04);
    out.extend_from_slice(&mac);
    debug_assert_eq!(out.len(), 0x2c);
    out
}

/// One second-stage claim (`02`, 50 bytes): the IP, the MAC, the number
/// being claimed, the counter, then `04 01`.
pub fn rekordbox_claim_stage2(mac: [u8; 6], ip: Ipv4Addr, number: u8, counter: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(0x32);
    write_header(&mut out, 0x02, 0x00, REKORDBOX_NAME);
    out.extend_from_slice(&[0x01, 0x03, 0x00, 0x32]);
    out.extend_from_slice(&ip.octets());
    out.extend_from_slice(&mac);
    out.push(number);
    out.push(counter);
    out.extend_from_slice(&[0x04, 0x01]);
    debug_assert_eq!(out.len(), 0x32);
    out
}

/// rekordbox's whole startup ladder in order: `00`×3, then `02` for each
/// reserved number with counter 1..=6.
pub fn rekordbox_startup_ladder(mac: [u8; 6], ip: Ipv4Addr) -> Vec<Vec<u8>> {
    let mut ladder = Vec::with_capacity(3 + REKORDBOX_CLAIM_NUMBERS.len() * 6);
    for counter in 1..=3 {
        ladder.push(rekordbox_claim_stage1(mac, counter));
    }
    for &number in &REKORDBOX_CLAIM_NUMBERS {
        for counter in 1..=6 {
            ladder.push(rekordbox_claim_stage2(mac, ip, number, counter));
        }
    }
    ladder
}

/// A player's question about one media slot on one device (`kind 05`, 48
/// bytes, unicast to port 50002): which device, which slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaQuery {
    pub name: String,
    /// The asking device's address, which the answer goes back to.
    pub from: Ipv4Addr,
    pub device_number: u8,
    pub slot: u8,
}

/// Byte length of a media query.
pub const MEDIA_QUERY_LEN: usize = 0x30;
/// The slot number a player uses for rekordbox's library.
pub const SLOT_REKORDBOX: u8 = 0x03;

impl MediaQuery {
    pub fn decode(packet: &[u8]) -> Result<Self> {
        if packet.len() < MEDIA_QUERY_LEN {
            return Err(PacketError::TooShort(packet.len()));
        }
        let kind = packet_kind(packet)?;
        if kind != 0x05 {
            return Err(PacketError::WrongKind(kind));
        }
        let at = |i: usize| packet.get(i).copied().unwrap_or(0);
        Ok(Self {
            name: status_device_name(packet)?,
            from: Ipv4Addr::new(at(0x24), at(0x25), at(0x26), at(0x27)),
            device_number: at(0x2b),
            slot: at(0x2f),
        })
    }
}

/// rekordbox's answer to a media query about its library (`kind 06`, 192
/// bytes, unicast back to the player's port 50002; measured 2026-09-12): the
/// name again in UTF-16BE, the track and playlist counts, and the fixed
/// bytes that say the tracks are rekordbox-analysed and there are settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaResponse {
    pub name: String,
    pub device_number: u8,
    pub tracks: u16,
    pub playlists: u16,
}

/// Byte length of a media response.
pub const MEDIA_RESPONSE_LEN: usize = 0xc0;

impl MediaResponse {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(MEDIA_RESPONSE_LEN);
        write_status_header(&mut out, 0x06, &self.name);
        out.extend_from_slice(&[0x01, 0x01, self.device_number]);
        out.extend_from_slice(&u16::try_from(MEDIA_RESPONSE_LEN - 0x24).unwrap_or(0).to_be_bytes());
        out.extend_from_slice(&u32::from(self.device_number).to_be_bytes());
        out.extend_from_slice(&u32::from(SLOT_REKORDBOX).to_be_bytes());
        // The name as the player shows it, UTF-16BE in a 64-byte field.
        let mut utf16: Vec<u8> = self.name.encode_utf16().take(31).flat_map(u16::to_be_bytes).collect();
        utf16.resize(0x40, 0);
        out.extend_from_slice(&utf16);
        // Creation date and the rest: zero for rekordbox.
        out.resize(0xa6, 0);
        out.extend_from_slice(&self.tracks.to_be_bytes());
        // Colour none; tracks are rekordbox's; settings present.
        out.extend_from_slice(&[0x00, 0x00, 0x01, 0x01, 0x00, 0x00]);
        out.extend_from_slice(&self.playlists.to_be_bytes());
        out.resize(MEDIA_RESPONSE_LEN, 0);
        debug_assert_eq!(out.len(), MEDIA_RESPONSE_LEN);
        out
    }
}

/// The kind of the 48-byte packet a player unicasts to port 50002 right
/// after the media response, whose meaning is unknown; rekordbox answers
/// it with [`link_handshake_reply`].
pub const LINK_HANDSHAKE_KIND: u8 = 0x46;

/// Byte length of the handshake reply.
pub const LINK_HANDSHAKE_REPLY_LEN: usize = 0x48;

/// rekordbox's reply to a `46` packet (`kind 47`, 72 bytes, unicast;
/// measured once, 2026-09-12). Every byte after the device number is copied
/// from the capture, meaning unknown.
pub fn link_handshake_reply(name: &str, device_number: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(LINK_HANDSHAKE_REPLY_LEN);
    write_status_header(&mut out, 0x47, name);
    out.extend_from_slice(&[0x01, 0x01, device_number, 0x00, 0x24, device_number, 0x04, 0x00, 0x00]);
    out.extend_from_slice(&[0x12, 0x34, 0x56, 0x78, 0x00, 0x00, 0x00, 0x01]);
    out.extend_from_slice(&[0x01, 0x01, 0x04, 0x01, 0x01, 0x01, 0x00, 0x00, 0x02]);
    out.resize(LINK_HANDSHAKE_REPLY_LEN, 0);
    debug_assert_eq!(out.len(), LINK_HANDSHAKE_REPLY_LEN);
    out
}

/// The kind of a player's status packet on port 50002.
pub const PLAYER_STATUS_KIND: u8 = 0x0a;

/// A device-number claim, sent three times in each of three stages at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub stage: AnnounceKind,
    pub name: String,
    pub device_number: u8,
    pub mac: [u8; 6],
    pub ip: Ipv4Addr,
    /// 1..=3, which of the three repeats this is.
    pub repeat: u8,
    pub device_type: DeviceType,
}

impl Claim {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(0x30);
        write_header(&mut out, self.stage.to_u8(), 0x00, &self.name);
        out.push(self.repeat);
        out.push(self.device_type.to_u8());
        // Stage 1 carries only the MAC; stages 2 and 3 add the address being
        // claimed, so they are four bytes longer and declare it.
        if self.stage == AnnounceKind::ClaimStage1 {
            out.extend_from_slice(&0x002c_u16.to_be_bytes());
            out.extend_from_slice(&self.mac);
        } else {
            out.extend_from_slice(&0x0032_u16.to_be_bytes());
            out.extend_from_slice(&self.ip.octets());
            out.extend_from_slice(&self.mac);
            out.push(self.device_number);
            out.push(0x01);
        }
        out
    }
}

/// The initial "I am here" announcement (`kind 0a`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    pub name: String,
    pub device_type: DeviceType,
}

impl Announcement {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(0x25);
        write_header(&mut out, AnnounceKind::Announce.to_u8(), 0x00, &self.name);
        out.push(0x01);
        out.push(self.device_type.to_u8());
        out.extend_from_slice(&0x0025_u16.to_be_bytes());
        out.push(0x01);
        out
    }
}

/// A device heard on the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub name: String,
    pub device_number: u8,
    pub device_type: DeviceType,
    pub ip: Ipv4Addr,
    /// Milliseconds since this device was last heard from.
    pub last_seen_ms: u64,
}

/// Tracks who is on the network.
///
/// A device that has gone quiet for three keep-alive intervals is dropped: a
/// player unplugged mid-set stops announcing rather than saying goodbye.
#[derive(Debug, Default)]
pub struct DeviceTable {
    peers: Vec<Peer>,
}

/// How long a peer may be silent before it is considered gone.
pub const PEER_TIMEOUT_MS: u64 = KEEP_ALIVE_INTERVAL_MS * 3;

impl DeviceTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a keep-alive at a given time.
    pub fn observe(&mut self, keep_alive: &KeepAlive, now_ms: u64) {
        if let Some(existing) = self
            .peers
            .iter_mut()
            .find(|p| p.device_number == keep_alive.device_number)
        {
            existing.name.clear();
            existing.name.push_str(&keep_alive.name);
            existing.device_type = keep_alive.device_type;
            existing.ip = keep_alive.ip;
            existing.last_seen_ms = now_ms;
            return;
        }
        self.peers.push(Peer {
            name: keep_alive.name.clone(),
            device_number: keep_alive.device_number,
            device_type: keep_alive.device_type,
            ip: keep_alive.ip,
            last_seen_ms: now_ms,
        });
    }

    /// Drops peers that have gone quiet, returning how many were removed.
    pub fn expire(&mut self, now_ms: u64) -> usize {
        let before = self.peers.len();
        self.peers
            .retain(|p| now_ms.saturating_sub(p.last_seen_ms) <= PEER_TIMEOUT_MS);
        before - self.peers.len()
    }

    pub fn peers(&self) -> &[Peer] {
        &self.peers
    }

    pub fn len(&self) -> usize {
        self.peers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    /// A device number not already in use, for claiming one at startup.
    pub fn free_device_number(&self, preferred: u8) -> u8 {
        if !self.peers.iter().any(|p| p.device_number == preferred) {
            return preferred;
        }
        // rekordbox-style numbers live above the player range.
        (0x11..=0x20)
            .find(|candidate| !self.peers.iter().any(|p| p.device_number == *candidate))
            .unwrap_or(preferred)
    }
}
