//! Byte-level tests for the database-server message codec.
//!
//! Everything here is checked against the encoding rules, not against a
//! capture: a capture-backed corpus is the next step and is tracked in TODO.md.
#![allow(clippy::pedantic, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rbl_dbserver::{kind, menu_footer, menu_header, setup_request, Argument, DbError, Message, MAGIC, SETUP_TXID};

/// Header is magic, txid, type, count, then the fixed twelve-byte tag list.
const HEADER_LEN: usize = 4 + 4 + 2 + 1 + 1 + 4 + 12;

#[test]
fn header_layout_is_exact() {
    let bytes = Message::new(0x1234_5678, kind::ROOT_MENU, vec![]).encode();
    assert_eq!(bytes.len(), HEADER_LEN, "an argument-free message is header only");
    assert_eq!(&bytes[0..4], &MAGIC.to_be_bytes());
    assert_eq!(&bytes[4..8], &0x1234_5678_u32.to_be_bytes());
    assert_eq!(&bytes[8..10], &kind::ROOT_MENU.to_be_bytes());
    assert_eq!(bytes[10], 0, "argument count");
    assert_eq!(bytes[11], 0x14, "the tag list is itself a blob field");
    assert_eq!(&bytes[12..16], &12_u32.to_be_bytes());
    assert_eq!(&bytes[16..28], &[0_u8; 12], "unused tag slots are zero");
}

#[test]
fn tag_list_stays_twelve_bytes_whatever_the_argument_count() {
    for count in 0..=3 {
        let args = (0..count).map(Argument::Number).collect::<Vec<_>>();
        let bytes = Message::new(1, kind::RENDER, args).encode();
        assert_eq!(bytes[10], u8::try_from(count).unwrap());
        // Slots up to the count carry 0x06 (number); the rest stay zero.
        for slot in 0..12 {
            let expected = if slot < count as usize { 0x06 } else { 0x00 };
            assert_eq!(bytes[16 + slot], expected, "slot {slot} with {count} arguments");
        }
    }
}

#[test]
fn argument_tags_differ_between_the_header_list_and_the_value() {
    let message = Message::new(
        7,
        kind::SEARCH,
        vec![Argument::String("a".into()), Argument::Blob(vec![9]), Argument::Number(5)],
    );
    let bytes = message.encode();
    // Header list: string 02, blob 03, number 06.
    assert_eq!(&bytes[16..19], &[0x02, 0x03, 0x06]);
    // First value field begins right after the header with the string tag 0x26.
    assert_eq!(bytes[HEADER_LEN], 0x26);
}

#[test]
fn round_trips_every_argument_kind() {
    let message = Message::new(
        42,
        kind::METADATA,
        vec![
            Argument::Number(0x0102_0304),
            Argument::String("Take Me Home".into()),
            Argument::Blob(vec![0, 1, 2, 253, 254, 255]),
            Argument::String(String::new()),
        ],
    );
    let bytes = message.encode();
    let (back, used) = Message::decode(&bytes).unwrap();
    assert_eq!(back, message);
    assert_eq!(used, bytes.len(), "decode consumes exactly the message");
}

#[test]
fn strings_are_utf16be_with_a_trailing_nul_counted_in_the_length() {
    let bytes = Message::new(1, kind::SEARCH, vec![Argument::String("Hi".into())]).encode();
    let field = &bytes[HEADER_LEN..];
    assert_eq!(field[0], 0x26);
    assert_eq!(&field[1..5], &3_u32.to_be_bytes(), "two characters plus the NUL");
    assert_eq!(&field[5..11], &[0x00, b'H', 0x00, b'i', 0x00, 0x00]);
}

#[test]
fn non_ascii_survives_the_utf16_round_trip() {
    // Includes a character outside the BMP, which is a surrogate pair in UTF-16.
    for text in ["Björk", "とんかつ", "Ø", "emoji 🎧 here"] {
        let message = Message::new(1, kind::SEARCH, vec![Argument::String(text.into())]);
        let (back, _) = Message::decode(&message.encode()).unwrap();
        assert_eq!(back, message, "{text}");
    }
}

#[test]
fn decodes_the_narrow_number_tags_a_player_may_send() {
    // We always emit 0x11 (four bytes), but a client may use 0x0f or 0x10.
    let mut bytes = Message::new(1, kind::RENDER, vec![]).encode();
    bytes[10] = 2;
    bytes[16] = 0x06;
    bytes[17] = 0x06;
    bytes.extend_from_slice(&[0x0f, 0x7b]); // one byte: 123
    bytes.extend_from_slice(&[0x10, 0x01, 0x00]); // two bytes: 256

    let (message, used) = Message::decode(&bytes).unwrap();
    assert_eq!(message.arguments, vec![Argument::Number(123), Argument::Number(256)]);
    assert_eq!(used, bytes.len());
}

#[test]
fn several_messages_in_one_segment_are_all_decoded() {
    let sent = vec![
        menu_header(3, 128),
        Message::new(3, kind::MENU_ITEM, vec![Argument::String("Melodic Vox".into())]),
        menu_footer(3),
    ];
    let mut wire = Vec::new();
    for message in &sent {
        wire.extend_from_slice(&message.encode());
    }

    let (got, used) = Message::decode_all(&wire);
    assert_eq!(got, sent);
    assert_eq!(used, wire.len());
}

#[test]
fn a_message_split_across_segments_is_left_for_the_next_read() {
    let whole = Message::new(9, kind::MENU_ITEM, vec![Argument::String("Tech House".into())]);
    let wire = {
        let mut w = menu_header(9, 1).encode();
        w.extend_from_slice(&whole.encode());
        w
    };
    let first = menu_header(9, 1).encode().len();

    // Every split point inside the second message must yield exactly one
    // complete message and leave the partial bytes behind.
    for cut in first + 1..wire.len() {
        let (got, used) = Message::decode_all(&wire[..cut]);
        assert_eq!(got.len(), 1, "cut at {cut}");
        assert_eq!(used, first, "cut at {cut} consumes only the whole message");

        // Feeding the remainder afterwards recovers the rest.
        let (rest, _) = Message::decode_all(&wire[used..]);
        assert_eq!(rest, vec![whole.clone()]);
    }
}

#[test]
fn truncation_is_an_error_rather_than_a_panic() {
    let bytes = Message::new(1, kind::METADATA, vec![Argument::String("Angels".into())]).encode();
    for cut in 0..bytes.len() {
        match Message::decode(&bytes[..cut]) {
            Err(DbError::Truncated { .. }) => {}
            other => panic!("cut at {cut} gave {other:?}"),
        }
    }
}

#[test]
fn a_blob_length_beyond_the_buffer_is_rejected() {
    let mut bytes = Message::new(1, kind::ANLZ_TAG, vec![]).encode();
    bytes[10] = 1;
    bytes[16] = 0x03;
    bytes.push(0x14);
    bytes.extend_from_slice(&0xffff_ffff_u32.to_be_bytes()); // lies about its size
    assert!(matches!(Message::decode(&bytes), Err(DbError::Truncated { .. })));
}

#[test]
fn a_string_length_beyond_the_buffer_is_rejected() {
    let mut bytes = Message::new(1, kind::SEARCH, vec![]).encode();
    bytes[10] = 1;
    bytes[16] = 0x02;
    bytes.push(0x26);
    bytes.extend_from_slice(&0x4000_0000_u32.to_be_bytes());
    // Must not overflow when the count is doubled to a byte length.
    assert!(matches!(Message::decode(&bytes), Err(DbError::Truncated { .. })));
}

#[test]
fn a_foreign_packet_is_not_mistaken_for_a_message() {
    let mut bytes = Message::new(1, kind::ROOT_MENU, vec![]).encode();
    bytes[0] = 0x51; // the Pro DJ Link magic starts here
    assert!(matches!(Message::decode(&bytes), Err(DbError::BadMagic(_))));
}

#[test]
fn an_impossible_argument_count_is_rejected() {
    let mut bytes = Message::new(1, kind::ROOT_MENU, vec![]).encode();
    bytes[10] = 13;
    assert_eq!(Message::decode(&bytes), Err(DbError::TooManyArguments(13)));
}

#[test]
fn an_unknown_field_tag_is_reported_not_guessed() {
    let mut bytes = Message::new(1, kind::ROOT_MENU, vec![]).encode();
    bytes[10] = 1;
    bytes[16] = 0x06;
    bytes.push(0x99);
    assert_eq!(Message::decode(&bytes), Err(DbError::UnknownTag(0x99)));
}

#[test]
fn decode_all_stops_at_garbage_rather_than_looping() {
    let mut wire = menu_header(1, 0).encode();
    wire.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef, 0x00, 0x11, 0x22]);
    let (got, used) = Message::decode_all(&wire);
    assert_eq!(got.len(), 1);
    assert_eq!(used, menu_header(1, 0).encode().len());
}

#[test]
fn setup_uses_the_reserved_transaction_id() {
    let message = setup_request(2);
    assert_eq!(message.transaction, SETUP_TXID);
    assert_eq!(message.kind, kind::SETUP);
    assert_eq!(message.arguments, vec![Argument::Number(2)]);
    assert_eq!(SETUP_TXID, 0xffff_fffe);
}

#[test]
fn only_device_numbers_one_to_six_are_answerable() {
    // A real server silently ignores 7 and above: the connection succeeds and
    // then nothing ever arrives, so we reject it where it can be explained.
    for device in 1..=6 {
        assert!(rbl_dbserver::is_answerable_device(device), "device {device}");
    }
    for device in [0_u8, 7, 8, 17, 255] {
        assert!(!rbl_dbserver::is_answerable_device(device), "device {device}");
    }
}

#[test]
fn the_port_query_string_is_nul_terminated() {
    assert_eq!(rbl_dbserver::PORT_QUERY_REQUEST, b"RemoteDBServer\0");
    assert_eq!(rbl_dbserver::PORT_QUERY, 12_523);
}

#[test]
fn what_still_needs_a_capture_is_recorded() {
    assert!(!rbl_dbserver::UNVERIFIED.is_empty());
}
