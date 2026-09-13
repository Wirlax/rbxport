//! The beacon against captured packets: a player's keep-alive, its media
//! query, its `46`, and its status with one of our tracks playing, all sent
//! from a socket standing in for the player, on loopback.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rbl_link::beacon::{Beacon, BeaconConfig, LibraryFacts};
use rbl_prolink::{packet_kind, DeviceType};

const CDJ_KEEP_ALIVE: &str =
    "5173707431576d4a4f4c060043444a2d333030300000000000000000000000000103003601012497ed0b4043c0a80198030000000164";
const MEDIA_QUERY: &str =
    "5173707431576d4a4f4c0543444a2d33303030000000000000000000000000010001000cc0a801980000001100000003";
const HANDSHAKE: &str = "5173707431576d4a4f4c4643444a2d333030300000000000000000000000000100010004010400e4";
const STATUS_PLAYING_OURS: &[u8] = include_bytes!("fixtures/cdj-status-playing-ours.bin");
const STATUS_EMPTY: &[u8] = include_bytes!("fixtures/cdj-status-empty.bin");

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

struct Facts;
impl LibraryFacts for Facts {
    fn track_count(&self) -> u16 {
        38_681
    }
    fn playlist_count(&self) -> u16 {
        627
    }
}

/// A socket standing in for the player, and the beacon told to answer it there.
fn start() -> (Beacon, UdpSocket) {
    let player = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    player.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let beacon = Beacon::start(
        BeaconConfig {
            address: Ipv4Addr::LOCALHOST,
            broadcast: Ipv4Addr::LOCALHOST,
            mac: [0x00, 0xe0, 0x4c, 0xcf, 0x63, 0x2e],
            announce_port: 0,
            status_port: 0,
            player_port: player.local_addr().unwrap().port(),
        },
        Arc::new(Facts),
    )
    .unwrap();
    (beacon, player)
}

/// Receives until a packet of `kind` arrives; the beacon's own broadcasts
/// are interleaved with the replies.
fn receive(player: &UdpSocket, kind: u8) -> Vec<u8> {
    let mut buffer = [0_u8; 2048];
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let Ok((len, _)) = player.recv_from(&mut buffer) else { continue };
        if packet_kind(&buffer[..len]) == Ok(kind) {
            return buffer[..len].to_vec();
        }
    }
    panic!("no packet of kind {kind:#x}");
}

fn wait_for(beacon: &Beacon, ready: impl Fn(&[rbl_link::Player]) -> bool) -> Vec<rbl_link::Player> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let players = beacon.players();
        if ready(&players) || Instant::now() > deadline {
            return players;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_player_is_listed_from_its_keep_alive_and_answered_on_its_status_port() {
    let (beacon, player) = start();
    let announce = SocketAddrV4::new(Ipv4Addr::LOCALHOST, beacon.announce_port());
    let status = SocketAddrV4::new(Ipv4Addr::LOCALHOST, beacon.status_port());

    player.send_to(&hex(CDJ_KEEP_ALIVE), announce).unwrap();
    let players = wait_for(&beacon, |p| !p.is_empty());
    assert_eq!(players.len(), 1);
    assert_eq!(players[0].name, "CDJ-3000");
    assert_eq!(players[0].number, 1);
    assert_eq!(players[0].kind, DeviceType::Cdj);
    assert_eq!(players[0].loaded, None);

    // The media query is answered with the library's counts. The captured
    // query names 192.168.1.152 as the asker; ours has to name us.
    let mut query = hex(MEDIA_QUERY);
    query[0x24..0x28].copy_from_slice(&Ipv4Addr::LOCALHOST.octets());
    player.send_to(&query, status).unwrap();
    let response = receive(&player, 0x06);
    assert_eq!(response.len(), 0xc0);
    assert_eq!(u16::from_be_bytes([response[0xa6], response[0xa7]]), 38_681);
    assert_eq!(u16::from_be_bytes([response[0xae], response[0xaf]]), 627);

    player.send_to(&hex(HANDSHAKE), status).unwrap();
    let reply = receive(&player, 0x47);
    assert_eq!(reply.len(), 0x48);
    assert_eq!(reply[0x21], 0x11);

    // The first status from the player earns the greeting; its content
    // says what it has loaded from us and whether it is master.
    player.send_to(STATUS_PLAYING_OURS, status).unwrap();
    let greeting = receive(&player, 0x16);
    assert_eq!(greeting.len(), 0x30);
    let players = wait_for(&beacon, |p| p[0].loaded.is_some());
    assert_eq!(players[0].loaded, Some(17_181));
    assert!(players[0].playing);
    assert!(players[0].master);
    assert_eq!(players[0].bpm_x100, 12_539);

    // Our status now carries the master's tempo.
    let ours = receive(&player, 0x29);
    assert_eq!(u16::from_be_bytes([ours[0x2e], ours[0x2f]]), 12_539);

    // Unloading clears it.
    player.send_to(STATUS_EMPTY, status).unwrap();
    let players = wait_for(&beacon, |p| p[0].loaded.is_none());
    assert_eq!(players[0].loaded, None);
    assert!(!players[0].playing);

    beacon.stop();
}

#[test]
fn the_status_beacon_runs_at_five_hertz_with_no_tempo_until_a_master_reports() {
    let (beacon, player) = start();
    let first = receive(&player, 0x29);
    let started = Instant::now();
    assert_eq!(first.len(), 0x38);
    assert_eq!(&first[0x0b..0x14], b"rekordbox");
    assert_eq!(first[0x21], 0x11);
    assert_eq!(u16::from_be_bytes([first[0x2e], first[0x2f]]), 0, "no master yet");
    // Four more within a second and a bit: 200 ms apart.
    for _ in 0..4 {
        receive(&player, 0x29);
    }
    assert!(started.elapsed() < Duration::from_millis(1200), "{:?}", started.elapsed());
    assert!(beacon.players().is_empty());
    beacon.stop();
}
