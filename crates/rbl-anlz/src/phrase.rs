//! `PSSI` — the song structure, which rekordbox draws as the phrase strip
//! above the waveform.
//!
//! The tag lives in the `.EXT` file. Its shape is documented by Deep Symmetry
//! [REF] and confirmed here against the reference library [OBS]:
//!
//! ```text
//! 0c  len_entry_bytes  u32   always 24
//! 10  len_entries      u16   phrase count
//! 12  ── body, sometimes XOR-masked ──
//!     +00  mood       u16   1 high, 2 mid, 3 low
//!     +02  unknown    6 bytes, zero in every file checked
//!     +08  end_beat   u16   where the last phrase ends
//!     +0a  unknown    2 bytes
//!     +0c  bank       u8    Lighting-mode stylistic bank
//!     +0d  unknown    1 byte
//!     +0e  entries, `len_entry_bytes` each
//! ```
//!
//! and each entry:
//!
//! ```text
//! +00 index u16   1-based
//! +02 beat  u16   first beat of the phrase
//! +04 kind  u16   meaning depends on the mood
//! +07 k1    u8  ┐
//! +09 k2    u8  ├ high-mood label variants
//! +13 k3    u8  ┘
//! +0b b     u8    0: one extra beat, 1: three (only seen on "UP 3")
//! +0c beat2 u16 ┐
//! +0e beat3 u16 ├ lighting hints inside the phrase, purpose [UNKNOWN]
//! +10 beat4 u16 ┘
//! +15 fill      u8    non-zero when the phrase ends with a fill-in
//! +16 fill_beat u16   where that fill-in starts
//! ```
//!
//! **The masking.** rekordbox 6 garbles everything from byte `0x12` with a
//! repeating 19-byte XOR pad, each byte of a fixed base plus the low byte of
//! `len_entries`. Files written by rekordbox 7 into the local share tree are
//! *not* masked — 0 of 600 checked carry the pad [OBS] — so both have to be
//! read. Which one a file is cannot be asked of the file, so it is decided by
//! whether the mood reads as 1, 2 or 3; that test cannot be fooled, because a
//! masked body only puts a zero in the mood's high byte when `len_entries` is
//! 53, and the low byte is then 0x16 XOR the mood, which is never 1, 2 or 3.

use crate::Section;
use rbl_core::FourCc;

/// The XOR pad rekordbox 6 masks the body with, before `len_entries` is added
/// to every byte.
const MASK_BASE: [u8; 19] = [
    0xCB, 0xE1, 0xEE, 0xFA, 0xE5, 0xEE, 0xAD, 0xEE, 0xE9, 0xD2, 0xE9, 0xEB, 0xE1, 0xE9, 0xF3, 0xE8,
    0xE9, 0xF4, 0xE1,
];

/// Bytes of the body before the first entry.
const BODY_PREAMBLE: usize = 14;

/// The smallest entry this can read every documented field out of.
const MIN_ENTRY_BYTES: usize = 24;

/// How rekordbox classified the track as a whole, which decides what the
/// per-phrase `kind` means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    High,
    Mid,
    Low,
}

impl Mood {
    fn from_u16(raw: u16) -> Option<Self> {
        match raw {
            1 => Some(Self::High),
            2 => Some(Self::Mid),
            3 => Some(Self::Low),
            _ => None,
        }
    }
}

/// One phrase of the song structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phrase {
    /// 1-based position in the tag.
    pub index: u16,
    /// The beat the phrase starts on, 1-based, matching the `PQTZ` grid.
    pub beat: u16,
    /// The raw `kind`. Only meaningful together with the mood.
    pub kind: u16,
    /// What rekordbox draws, e.g. `INTRO 2`, `UP 3`, `VERSE 1`. Empty for a
    /// `kind` no mood defines, rather than a made-up name.
    pub label: &'static str,
    /// A fill-in at the end of the phrase, which rekordbox dots on the
    /// waveform.
    pub fill_in: bool,
    /// The beat that fill-in starts on. Zero when there is none.
    pub fill_in_beat: u16,
}

/// A parsed `PSSI`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongStructure {
    pub mood: Mood,
    /// The beat the last phrase ends on. Audio may continue past it.
    pub end_beat: u16,
    /// The Lighting-mode bank the user assigned. 0 in every file checked,
    /// meaning no assignment [OBS]; values above 8 have been seen elsewhere,
    /// so it is kept raw rather than turned into an enum.
    pub bank: u8,
    pub phrases: Vec<Phrase>,
}

/// The label rekordbox shows for a `kind` in a given mood.
///
/// The high mood's numbered variants are not in `kind` at all: they come from
/// three flag bytes, in the combinations tabulated by Deep Symmetry [REF].
/// Every one of the 8,488 high-mood phrases in the reference library uses one
/// of those combinations and no other [OBS].
fn label_for(mood: Mood, kind: u16, k1: u8, k2: u8, k3: u8) -> &'static str {
    match mood {
        Mood::High => match (kind, k1, k2, k3) {
            (1, 1, _, _) => "INTRO 1",
            (1, _, _, _) => "INTRO 2",
            (2, _, 0, 0) => "UP 1",
            (2, _, 0, _) => "UP 2",
            (2, _, _, _) => "UP 3",
            (3, ..) => "DOWN",
            (5, 1, _, _) => "CHORUS 1",
            (5, ..) => "CHORUS 2",
            (6, 1, _, _) => "OUTRO 1",
            (6, ..) => "OUTRO 2",
            _ => "",
        },
        // rekordbox draws no distinction between the three ids it has for
        // each of "Verse 1" and "Verse 2" in the low mood.
        Mood::Low => match kind {
            1 => "INTRO",
            2..=4 => "VERSE 1",
            5..=7 => "VERSE 2",
            8 => "BRIDGE",
            9 => "CHORUS",
            10 => "OUTRO",
            _ => "",
        },
        Mood::Mid => match kind {
            1 => "INTRO",
            2 => "VERSE 1",
            3 => "VERSE 2",
            4 => "VERSE 3",
            5 => "VERSE 4",
            6 => "VERSE 5",
            7 => "VERSE 6",
            8 => "BRIDGE",
            9 => "CHORUS",
            10 => "OUTRO",
            _ => "",
        },
    }
}

fn be16(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([
        bytes.get(at).copied().unwrap_or(0),
        bytes.get(at + 1).copied().unwrap_or(0),
    ])
}

fn be32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([
        bytes.get(at).copied().unwrap_or(0),
        bytes.get(at + 1).copied().unwrap_or(0),
        bytes.get(at + 2).copied().unwrap_or(0),
        bytes.get(at + 3).copied().unwrap_or(0),
    ])
}

/// Undoes the XOR pad in place.
fn unmask(body: &mut [u8], len_entries: u16) {
    // The pad's period does not divide a `u16` boundary, so the offset within
    // it is the offset within the body — not within any chunk.
    #[allow(clippy::cast_possible_truncation)]
    let add = len_entries as u8;
    for (at, byte) in body.iter_mut().enumerate() {
        let pad = MASK_BASE
            .get(at % MASK_BASE.len())
            .copied()
            .unwrap_or(0)
            .wrapping_add(add);
        *byte ^= pad;
    }
}

impl Section {
    /// `PSSI` — the song structure, unmasked if it needed it.
    ///
    /// `None` for any other tag, and for a `PSSI` too short or too damaged to
    /// read: a garbled tag must cost the phrase strip, not the track.
    #[must_use]
    pub fn as_song_structure(&self) -> Option<SongStructure> {
        if self.tag != FourCc::new(b"PSSI") {
            return None;
        }
        // `len_header` is 20 in the exported files the spec describes and 32 in
        // the ones rekordbox 7 writes locally, which puts the same fields on
        // either side of the header/payload split. Joining them back up is what
        // makes one set of offsets serve both.
        let mut raw = Vec::with_capacity(self.header.len() + self.payload.len());
        raw.extend_from_slice(&self.header);
        raw.extend_from_slice(&self.payload);

        let len_entry_bytes = be32(&raw, 0) as usize;
        let len_entries = be16(&raw, 4);
        if len_entry_bytes < MIN_ENTRY_BYTES {
            return None;
        }

        let mut body = raw.get(6..)?.to_vec();
        let mood = if let Some(mood) = Mood::from_u16(be16(&body, 0)) {
            mood
        } else {
            unmask(&mut body, len_entries);
            Mood::from_u16(be16(&body, 0))?
        };

        let entries = body.get(BODY_PREAMBLE..)?;
        // A count the payload cannot back is a damaged tag; read what is
        // there rather than trusting the header or refusing the whole tag.
        let available = entries.len() / len_entry_bytes;
        let count = (len_entries as usize).min(available);

        let mut phrases = Vec::with_capacity(count);
        for entry in entries.chunks_exact(len_entry_bytes).take(count) {
            let kind = be16(entry, 4);
            let k1 = entry.get(0x07).copied().unwrap_or(0);
            let k2 = entry.get(0x09).copied().unwrap_or(0);
            let k3 = entry.get(0x13).copied().unwrap_or(0);
            phrases.push(Phrase {
                index: be16(entry, 0),
                beat: be16(entry, 2),
                kind,
                label: label_for(mood, kind, k1, k2, k3),
                fill_in: entry.get(0x15).copied().unwrap_or(0) != 0,
                fill_in_beat: be16(entry, 0x16),
            });
        }

        Some(SongStructure {
            mood,
            end_beat: be16(&body, 8),
            bank: body.get(0x0c).copied().unwrap_or(0),
            phrases,
        })
    }
}

impl crate::Anlz {
    /// The song structure, if the file carries one.
    #[must_use]
    pub fn song_structure(&self) -> Option<SongStructure> {
        self.sections.iter().find_map(Section::as_song_structure)
    }

    /// The phrases alone, which is what the waveform strip draws.
    #[must_use]
    pub fn phrases(&self) -> Option<Vec<Phrase>> {
        self.song_structure().map(|s| s.phrases)
    }
}
