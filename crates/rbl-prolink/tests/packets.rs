//! Packet encoding, decoding, and the device table's expiry rules.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::Ipv4Addr;

use rbl_prolink::{
    device_name, packet_kind, AnnounceKind, Announcement, Claim, DeviceTable, DeviceType,
    KeepAlive, PacketError, KEEP_ALIVE_LEN, MAGIC, PEER_TIMEOUT_MS, REKORDBOX_DEVICE_NUMBER,
    REKORDBOX_NAME,
};

fn sample() -> KeepAlive {
    KeepAlive::rekordbox([0x00, 0x1e, 0x1d, 0x11, 0x22, 0x33], Ipv4Addr::new(192, 168, 1, 42), 2)
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

/// rekordbox 7.2.11's keep-alive with one player on the network, verbatim
/// from the capture of 2026-09-12 (MAC 00:e0:4c:cf:63:2e, 192.168.1.14).
const CAPTURED_REKORDBOX_KEEP_ALIVE: &str =
    "5173707431576d4a4f4c060072656b6f7264626f78000000000000000000000001030036110100e04ccf632ec0a8010e020100000408";
/// The CDJ-3000's, from the same capture (player 1, 192.168.1.152).
const CAPTURED_CDJ_KEEP_ALIVE: &str =
    "5173707431576d4a4f4c060043444a2d333030300000000000000000000000000103003601012497ed0b4043c0a80198030000000164";
/// rekordbox's status beacon while the deck was master at 78.08 BPM.
const CAPTURED_REKORDBOX_STATUS: &str =
    "5173707431576d4a4f4c2972656b6f7264626f7800000000000000000000000101110038110000c00010000080001e80001000000009ff01";
/// The player's question about rekordbox's library slot, and rekordbox's
/// answer (38,681 tracks, 627 playlists), then the `46` packet and its
/// `47` reply, all from the same capture.
const CAPTURED_MEDIA_QUERY: &str =
    "5173707431576d4a4f4c0543444a2d33303030000000000000000000000000010001000cc0a801980000001100000003";
const CAPTURED_MEDIA_RESPONSE: &str =
    "5173707431576d4a4f4c0672656b6f7264626f780000000000000000000000010111009c000000110000000300720065006b006f007200640062006f007800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000009719000001010000027300000000000000000000000000000000";
const CAPTURED_HANDSHAKE_REPLY: &str =
    "5173707431576d4a4f4c4772656b6f7264626f7800000000000000000000000101110024110400001234567800000001010104010101000002000000000000000000000000000000";
/// The packet rekordbox unicasts to a player that has just connected.
const CAPTURED_CONNECT_GREETING: &str =
    "5173707431576d4a4f4c1672656b6f7264626f7800000000000000000000000101110000000000000000000000000000";
/// rekordbox's answer (`11`) to a player's `10` identity packet, verbatim
/// from the 2026-09-13 capture on the emulator bridge (computer "chrisles-MBP",
/// UTF-16BE), zero-padded to 296 bytes. Without this the player never lists us.
const CAPTURED_CONNECT_IDENTITY: &str = "5173707431576d4a4f4c1172656b6f7264626f78000000000000000000000001011101041101000000630068007200690073006c00650073002d004d0042005000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

#[test]
fn rekordboxs_keep_alive_is_reproduced_byte_for_byte() {
    let alive = KeepAlive::rekordbox([0x00, 0xe0, 0x4c, 0xcf, 0x63, 0x2e], Ipv4Addr::new(192, 168, 1, 14), 2);
    assert_eq!(alive.encode(), hex(CAPTURED_REKORDBOX_KEEP_ALIVE));
    assert_eq!(KeepAlive::decode(&hex(CAPTURED_REKORDBOX_KEEP_ALIVE)).unwrap(), alive);
}

#[test]
fn a_cdj_3000s_keep_alive_decodes_to_a_player() {
    let deck = KeepAlive::decode(&hex(CAPTURED_CDJ_KEEP_ALIVE)).unwrap();
    assert_eq!(deck.name, "CDJ-3000");
    assert_eq!(deck.device_number, 1);
    assert_eq!(deck.device_type, DeviceType::Cdj);
    assert_eq!(deck.ip, Ipv4Addr::new(192, 168, 1, 152));
    assert_eq!(deck.peers, 3);
    assert_eq!(deck.generation, 3);
    assert_eq!(deck.encode(), hex(CAPTURED_CDJ_KEEP_ALIVE));
}

#[test]
fn the_status_beacon_and_the_connect_greeting_match_the_capture() {
    let status = rbl_prolink::Status {
        name: REKORDBOX_NAME.to_owned(),
        device_number: REKORDBOX_DEVICE_NUMBER,
        bpm_x100: 0x1e80,
        beat: 1,
    };
    assert_eq!(status.encode(), hex(CAPTURED_REKORDBOX_STATUS));
    assert_eq!(
        rbl_prolink::connect_greeting(REKORDBOX_NAME, REKORDBOX_DEVICE_NUMBER),
        hex(CAPTURED_CONNECT_GREETING)
    );
}

#[test]
fn the_identity_reply_matches_the_capture() {
    let identity = rbl_prolink::connect_identity(REKORDBOX_NAME, REKORDBOX_DEVICE_NUMBER, "chrisles-MBP");
    assert_eq!(identity.len(), rbl_prolink::CONNECT_IDENTITY_LEN);
    assert_eq!(identity, hex(CAPTURED_CONNECT_IDENTITY));
}

#[test]
fn a_keep_alive_round_trips() {
    let original = sample();
    let bytes = original.encode();
    assert_eq!(bytes.len(), KEEP_ALIVE_LEN);
    assert_eq!(KeepAlive::decode(&bytes).unwrap(), original);
}

#[test]
fn every_packet_starts_with_the_dj_link_magic() {
    for bytes in [
        sample().encode(),
        Announcement { name: "rekordbox".into(), device_type: DeviceType::Rekordbox }.encode(),
        Claim {
            stage: AnnounceKind::ClaimStage1,
            name: "rekordbox".into(),
            device_number: 17,
            mac: [1, 2, 3, 4, 5, 6],
            ip: Ipv4Addr::LOCALHOST,
            repeat: 1,
            device_type: DeviceType::Rekordbox,
        }
        .encode(),
    ] {
        assert_eq!(bytes.get(0..10), Some(&MAGIC[..]), "magic missing");
    }
}

#[test]
fn the_device_name_sits_at_a_fixed_offset_and_is_nul_padded() {
    let bytes = sample().encode();
    assert_eq!(device_name(&bytes).unwrap(), "rekordbox");
    // Twenty bytes, so the field is padded rather than truncated.
    assert_eq!(&bytes[0x0c..0x0c + 9], b"rekordbox");
    assert!(bytes[0x15..0x20].iter().all(|&b| b == 0), "name must be NUL-padded");
}

#[test]
fn a_long_name_is_truncated_to_the_field_rather_than_overflowing() {
    let mut alive = sample();
    alive.name = "a-very-long-device-name-that-will-not-fit".into();
    let bytes = alive.encode();
    assert_eq!(bytes.len(), KEEP_ALIVE_LEN, "the packet must stay a fixed size");
    assert_eq!(device_name(&bytes).unwrap().len(), 20);
}

#[test]
fn keep_alive_fields_land_where_the_protocol_says() {
    let bytes = sample().encode();
    assert_eq!(bytes[0x0a], 0x06, "kind: keep-alive");
    assert_eq!(bytes[0x21], 0x03, "generation");
    assert_eq!(bytes[0x34], DeviceType::Rekordbox.to_u8());
    assert_eq!(u16::from_be_bytes([bytes[0x22], bytes[0x23]]) as usize, KEEP_ALIVE_LEN);
    assert_eq!(bytes[0x24], REKORDBOX_DEVICE_NUMBER);
    assert_eq!(&bytes[0x26..0x2c], &[0x00, 0x1e, 0x1d, 0x11, 0x22, 0x33]);
    assert_eq!(&bytes[0x2c..0x30], &[192, 168, 1, 42]);
    assert_eq!(bytes[0x30], 2, "peer count");
}

#[test]
fn rubbish_is_rejected_rather_than_misread() {
    assert_eq!(KeepAlive::decode(&[]), Err(PacketError::TooShort(0)));
    assert_eq!(packet_kind(&[0; 4]), Err(PacketError::TooShort(4)));

    let mut wrong = sample().encode();
    wrong[0] = 0xff;
    assert_eq!(packet_kind(&wrong), Err(PacketError::BadMagic));
    assert_eq!(device_name(&wrong), Err(PacketError::BadMagic));
}

#[test]
fn a_packet_of_the_wrong_kind_is_refused() {
    let mut bytes = sample().encode();
    bytes[0x0a] = AnnounceKind::Announce.to_u8();
    assert_eq!(KeepAlive::decode(&bytes), Err(PacketError::WrongKind(0x0a)));
}

#[test]
fn announce_kinds_round_trip() {
    for kind in [
        AnnounceKind::ClaimStage1,
        AnnounceKind::ClaimStage2,
        AnnounceKind::ClaimFinal,
        AnnounceKind::KeepAlive,
        AnnounceKind::Conflict,
        AnnounceKind::Announce,
        AnnounceKind::Other(0x7f),
    ] {
        assert_eq!(AnnounceKind::from_u8(kind.to_u8()), kind);
    }
}

#[test]
fn device_types_round_trip() {
    for kind in [DeviceType::Cdj, DeviceType::Mixer, DeviceType::Rekordbox, DeviceType::Other(9)] {
        assert_eq!(DeviceType::from_u8(kind.to_u8()), kind);
    }
}

#[test]
fn the_first_claim_stage_is_shorter_than_the_later_ones() {
    let base = Claim {
        stage: AnnounceKind::ClaimStage1,
        name: "rekordbox".into(),
        device_number: 17,
        mac: [1, 2, 3, 4, 5, 6],
        ip: Ipv4Addr::new(10, 0, 0, 5),
        repeat: 1,
        device_type: DeviceType::Rekordbox,
    };
    let first = base.encode();
    let second = Claim { stage: AnnounceKind::ClaimStage2, ..base.clone() }.encode();
    assert!(second.len() > first.len(), "later stages carry the IP and number");
    assert_eq!(first[0x0a], 0x00);
    assert_eq!(second[0x0a], 0x02);
    // The device number being claimed appears in the later stages.
    assert!(second.contains(&17));
}

// ---- device table ----

#[test]
fn the_table_records_and_updates_a_peer() {
    let mut table = DeviceTable::new();
    table.observe(&sample(), 1_000);
    assert_eq!(table.len(), 1);
    assert_eq!(table.peers()[0].name, "rekordbox");

    // A second keep-alive from the same device updates rather than duplicates.
    let mut moved = sample();
    moved.ip = Ipv4Addr::new(192, 168, 1, 99);
    table.observe(&moved, 2_000);
    assert_eq!(table.len(), 1);
    assert_eq!(table.peers()[0].ip, Ipv4Addr::new(192, 168, 1, 99));
    assert_eq!(table.peers()[0].last_seen_ms, 2_000);
}

#[test]
fn a_peer_that_goes_quiet_is_dropped() {
    let mut table = DeviceTable::new();
    table.observe(&sample(), 0);
    assert_eq!(table.expire(PEER_TIMEOUT_MS), 0, "still within the timeout");
    assert_eq!(table.len(), 1);
    assert_eq!(table.expire(PEER_TIMEOUT_MS + 1), 1, "a player unplugged mid-set stops announcing");
    assert!(table.is_empty());
}

#[test]
fn several_devices_are_tracked_separately() {
    let mut table = DeviceTable::new();
    for number in [1, 2, 17] {
        let mut alive = sample();
        alive.device_number = number;
        alive.name = format!("device-{number}");
        table.observe(&alive, 0);
    }
    assert_eq!(table.len(), 3);
    assert_eq!(table.free_device_number(17), 18, "17 is taken, so pick the next free one");
    assert_eq!(table.free_device_number(20), 20, "20 is free");
}

#[test]
fn an_empty_table_hands_back_the_preferred_number() {
    assert_eq!(DeviceTable::new().free_device_number(REKORDBOX_DEVICE_NUMBER), REKORDBOX_DEVICE_NUMBER);
}

#[test]
fn the_media_query_decodes_and_the_response_matches_the_capture() {
    let query = rbl_prolink::MediaQuery::decode(&hex(CAPTURED_MEDIA_QUERY)).unwrap();
    assert_eq!(query.name, "CDJ-3000");
    assert_eq!(query.from, Ipv4Addr::new(192, 168, 1, 152));
    assert_eq!(query.device_number, REKORDBOX_DEVICE_NUMBER);
    assert_eq!(query.slot, rbl_prolink::SLOT_REKORDBOX);

    let response = rbl_prolink::MediaResponse {
        name: REKORDBOX_NAME.to_owned(),
        device_number: REKORDBOX_DEVICE_NUMBER,
        tracks: 38_681,
        playlists: 627,
    };
    assert_eq!(response.encode(), hex(CAPTURED_MEDIA_RESPONSE));
}

#[test]
fn the_link_handshake_reply_matches_the_capture() {
    assert_eq!(
        rbl_prolink::link_handshake_reply(REKORDBOX_NAME, REKORDBOX_DEVICE_NUMBER),
        hex(CAPTURED_HANDSHAKE_REPLY)
    );
}

#[test]
fn the_rekordbox_startup_ladder_matches_the_capture() {
    let mac = [0x00, 0xe0, 0x4c, 0xcf, 0x63, 0x2e];
    let ip = Ipv4Addr::new(192, 168, 1, 14);
    assert_eq!(
        rbl_prolink::rekordbox_claim_stage1(mac, 1),
        hex("5173707431576d4a4f4c000072656b6f7264626f7800000000000000000000000103002c010400e04ccf632e")
    );
    assert_eq!(
        rbl_prolink::rekordbox_claim_stage2(mac, ip, 0x11, 1),
        hex("5173707431576d4a4f4c020072656b6f7264626f78000000000000000000000001030032c0a8010e00e04ccf632e11010401")
    );
    assert_eq!(
        rbl_prolink::rekordbox_claim_stage2(mac, ip, 0x2c, 6),
        hex("5173707431576d4a4f4c020072656b6f7264626f78000000000000000000000001030032c0a8010e00e04ccf632e2c060401")
    );
    // 3 first-stage + 6 numbers × 6 counters.
    assert_eq!(rbl_prolink::rekordbox_startup_ladder(mac, ip).len(), 3 + 36);
}

/// rekordbox's status with nothing loaded (cold-start capture, 2026-09-12):
/// the master byte is 0x00 and the beat is 0.
const CAPTURED_REKORDBOX_STATUS_IDLE: &str =
    "5173707431576d4a4f4c2972656b6f7264626f7800000000000000000000000101110038110000c00010000000000000001000000009ff00";

#[test]
fn the_idle_status_matches_the_capture() {
    let status = rbl_prolink::Status {
        name: REKORDBOX_NAME.to_owned(),
        device_number: REKORDBOX_DEVICE_NUMBER,
        bpm_x100: 0,
        beat: 0,
    };
    assert_eq!(status.encode(), hex(CAPTURED_REKORDBOX_STATUS_IDLE));
}
