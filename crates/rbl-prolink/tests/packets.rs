//! Packet encoding, decoding, and the device table's expiry rules.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::Ipv4Addr;

use rbl_prolink::{
    device_name, packet_kind, AnnounceKind, Announcement, Claim, DeviceTable, DeviceType,
    KeepAlive, PacketError, KEEP_ALIVE_LEN, MAGIC, PEER_TIMEOUT_MS, REKORDBOX_DEVICE_NUMBER,
    REKORDBOX_NAME,
};

fn sample() -> KeepAlive {
    KeepAlive {
        name: REKORDBOX_NAME.to_owned(),
        device_number: REKORDBOX_DEVICE_NUMBER,
        device_type: DeviceType::Rekordbox,
        mac: [0x00, 0x1e, 0x1d, 0x11, 0x22, 0x33],
        ip: Ipv4Addr::new(192, 168, 1, 42),
        peers: 2,
        was_first: false,
    }
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
    assert_eq!(bytes[0x21], DeviceType::Rekordbox.to_u8());
    assert_eq!(u16::from_be_bytes([bytes[0x22], bytes[0x23]]) as usize, KEEP_ALIVE_LEN);
    assert_eq!(bytes[0x24], REKORDBOX_DEVICE_NUMBER);
    assert_eq!(&bytes[0x26..0x2c], &[0x00, 0x1e, 0x1d, 0x11, 0x22, 0x33]);
    assert_eq!(&bytes[0x2c..0x30], &[192, 168, 1, 42]);
    assert_eq!(bytes[0x30], 2, "peer count");
}

#[test]
fn the_first_on_network_flag_is_carried_both_ways() {
    let mut alive = sample();
    alive.was_first = true;
    assert_eq!(alive.encode()[0x25], 0x02);
    assert!(KeepAlive::decode(&alive.encode()).unwrap().was_first);

    alive.was_first = false;
    assert_eq!(alive.encode()[0x25], 0x01);
    assert!(!KeepAlive::decode(&alive.encode()).unwrap().was_first);
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
