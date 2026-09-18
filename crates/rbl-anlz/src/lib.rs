//! Reader and writer for rekordbox ANLZ analysis files (`.DAT`, `.EXT`, `.2EX`).
//!
//! A file is a `PMAI` header followed by type-tagged sections. Each section is
//! `fourcc`, `len_header`, `len_tag`, then **tag-specific header fields up to
//! `len_header`**, then the payload.
//!
//! That middle part is easy to get wrong: `len_header` differs per tag (16 for
//! `PPTH`, 20 for `PWAV`, 24 for `PQTZ`, 14 for `PWVC`), and the fields it holds
//! are the ones a naive reader expects to find at the start of the body. Reading
//! them from the body instead silently consumes the first bytes of real data.
//!
//! To make that impossible, every section keeps its raw framing — the header
//! bytes and the payload exactly as they appeared — and decoded values are
//! derived from those. Re-emitting a parsed file therefore reproduces it byte
//! for byte, including tags we cannot author ourselves.

pub mod encode;
pub mod grid;
pub mod phrase;
pub mod vocal;
pub mod write;

use std::path::{Path, PathBuf};

use rbl_core::FourCc;

#[derive(Debug, thiserror::Error)]
pub enum AnlzError {
    #[error("not an ANLZ file: expected a PMAI header")]
    NotAnlz,
    #[error("file is truncated: {0}")]
    Truncated(&'static str),
    #[error("section {tag} declares {declared} bytes but only {available} remain")]
    BadSectionLength { tag: String, declared: u64, available: u64 },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, AnlzError>;

pub use phrase::{Mood, Phrase, PhraseEdit, SongStructure};
pub use vocal::{VOCAL_FRAME_MS, VOCAL_MAX};
pub use encode::{author, AnalysisFiles, BandColumn, Existing};
pub use write::AnlzBuilder;

/// Bytes of section framing before the tag-specific header fields.
pub const SECTION_FRAME: usize = 12;

/// A beat in the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Beat {
    /// 1..=4, where 1 is the downbeat.
    pub beat_number: u16,
    /// BPM x100 at this beat.
    pub tempo_x100: u16,
    /// Milliseconds from the start at 100% pitch.
    pub time_ms: u32,
}

/// One section, with its framing preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub tag: FourCc,
    /// Tag-specific header fields: the bytes between the 12-byte frame and
    /// `len_header`. Their meaning depends on the tag.
    pub header: Vec<u8>,
    /// Everything from `len_header` to `len_tag`.
    pub payload: Vec<u8>,
}

impl Section {
    pub fn new(tag: &[u8; 4], header: Vec<u8>, payload: Vec<u8>) -> Self {
        Self { tag: FourCc::new(tag), header, payload }
    }

    /// `len_header` as written: the frame plus the tag header.
    pub fn len_header(&self) -> u32 {
        u32::try_from(SECTION_FRAME + self.header.len()).unwrap_or(u32::MAX)
    }

    /// `len_tag` as written: everything, including the frame.
    pub fn len_tag(&self) -> u32 {
        u32::try_from(SECTION_FRAME + self.header.len() + self.payload.len()).unwrap_or(u32::MAX)
    }

    fn header_u4(&self, at: usize) -> u32 {
        let b = &self.header;
        u32::from_be_bytes([
            b.get(at).copied().unwrap_or(0),
            b.get(at + 1).copied().unwrap_or(0),
            b.get(at + 2).copied().unwrap_or(0),
            b.get(at + 3).copied().unwrap_or(0),
        ])
    }

    /// `PPTH` — the audio file this analysis describes.
    pub fn as_path(&self) -> Option<String> {
        (self.tag == FourCc::new(b"PPTH")).then(|| utf16be_to_string(&self.payload))
    }

    /// `PQTZ` — the beat grid.
    pub fn as_beat_grid(&self) -> Option<Vec<Beat>> {
        if self.tag != FourCc::new(b"PQTZ") {
            return None;
        }
        // The beat count is the last field of the tag header.
        let count = self.header_u4(8) as usize;
        let mut beats = Vec::with_capacity(count.min(self.payload.len() / 8));
        for chunk in self.payload.chunks_exact(8).take(count) {
            beats.push(Beat {
                beat_number: u16::from_be_bytes([chunk[0], chunk[1]]),
                tempo_x100: u16::from_be_bytes([chunk[2], chunk[3]]),
                time_ms: u32::from_be_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]),
            });
        }
        Some(beats)
    }

    /// Bytes per waveform column, where the tag carries one.
    pub fn waveform_stride(&self) -> Option<u32> {
        match &self.tag.0 {
            // A single byte per column; the header holds a length and flags.
            b"PWAV" | b"PWV2" | b"PWV6" => Some(1),
            b"PWV3" | b"PWV4" | b"PWV5" | b"PWV7" => Some(self.header_u4(0).max(1)),
            _ => None,
        }
    }

    /// Waveform payload, if this section holds one.
    pub fn waveform(&self) -> Option<&[u8]> {
        self.waveform_stride().map(|_| self.payload.as_slice())
    }

    /// True when this is a cue list. Share-tree cue lists are always empty:
    /// cues live in `djmdCue`. Verified across the whole library.
    pub fn is_cue_list(&self) -> bool {
        matches!(&self.tag.0, b"PCOB" | b"PCO2")
    }

    /// `PCO2` — the extended cue list, entry by entry.
    ///
    /// Empty in the share tree, where the cue list is a header and nothing
    /// else, and populated in an export, where rekordbox writes the cues it
    /// wants a player to draw. Only the extended list is read: the older
    /// `PCOB`/`PCPT` form carries no colour, which is the only reason to read
    /// one of these at all.
    pub fn as_cue_entries(&self) -> Option<Vec<CueEntry>> {
        if self.tag != FourCc::new(b"PCO2") {
            return None;
        }
        let b = &self.payload;
        let mut entries = Vec::new();
        let mut at = 0usize;
        while at + CUE_ENTRY_MIN <= b.len() {
            if b.get(at..at + 4) != Some(b"PCP2") {
                break;
            }
            let len_entry = be32(b, at + 8) as usize;
            if len_entry < CUE_ENTRY_MIN || at + len_entry > b.len() {
                break;
            }
            let e = b.get(at..at + len_entry).unwrap_or_default();
            entries.push(cue_entry(e, len_entry));
            at += len_entry;
        }
        Some(entries)
    }
}

/// One entry of an extended cue list.
///
/// The colour is the point: an entry rekordbox wrote carries both the index it
/// stores in `djmdCue.ColorTableIndex` and the RGB it paints for that index,
/// which is the only place the two have been seen side by side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueEntry {
    /// Zero for a memory cue, otherwise the hot cue's slot — 1 is A.
    pub hot_cue: u32,
    /// 1 is a cue, 2 a loop.
    pub kind: u8,
    pub time_ms: u32,
    /// Where a loop returns to. Only meaningful on a loop.
    pub loop_time_ms: u32,
    /// The colour a *memory* cue or loop was given, as a row of the colour
    /// table. Hot cues use `color_code` instead.
    pub color_id: u8,
    pub comment: Option<String>,
    /// `djmdCue.ColorTableIndex`.
    pub color_code: Option<u8>,
    /// What rekordbox paints for that index.
    pub rgb: Option<[u8; 3]>,
}

/// Magic, the two lengths, the hot cue, the kind, the two times, the colour
/// row and its eleven trailing bytes: an entry cannot be shorter than this.
const CUE_ENTRY_MIN: usize = 40;

/// What `djmdCue.ColorTableIndex` paints, where it has been read.
///
/// Measured, not guessed: rekordbox writes the index and the RGB side by side
/// in the `PCO2` entries of an export, and these are the pairs a real export
/// of 77 tracks carried. The same export cross-checks against `djmdCue` — 76
/// of the 77 tracks store exactly the indices the export drew — which is what
/// makes `color_code` and `ColorTableIndex` the same number rather than two
/// that happen to look alike. `cargo run -p rbl-db --example cue_colours` is
/// the probe.
///
/// **Incomplete, deliberately.** Eight more indices are in use in the
/// reference library and are not here, because no exported track carried one;
/// the probe names a track for each. An index that is not in this table has
/// not been read, and must not be guessed — `cue_colour` returns `None` for it
/// so a caller falls back rather than paints a wrong colour.
///
/// Note that this is what rekordbox *stores*, not always what it *draws*: the
/// hot-cue badge measured off a screenshot for index 21 is `#77E866`, a
/// lightened version of the `#00FF00` here.
pub const MEASURED_CUE_COLOURS: &[(u8, [u8; 3])] = &[
    (1, [0x00, 0x00, 0xFF]),
    (6, [0x00, 0x8C, 0xFF]),
    (18, [0x00, 0xFF, 0x47]),
    (21, [0x00, 0xFF, 0x00]),
    (25, [0x66, 0xFF, 0x00]),
    (33, [0xFF, 0xD1, 0x00]),
    (36, [0xFF, 0x8C, 0x00]),
    (46, [0xFF, 0x00, 0x5C]),
    (60, [0x4D, 0x00, 0xFF]),
];

/// The RGB for a `ColorTableIndex`, or `None` where it has not been read.
pub fn cue_colour(index: u8) -> Option<[u8; 3]> {
    MEASURED_CUE_COLOURS.iter().find(|&&(i, _)| i == index).map(|&(_, rgb)| rgb)
}

/// What rekordbox *paints* for a `ColorTableIndex` on screen, which is not
/// what it stores.
///
/// `[OBS]` rekordbox 7.2.11, `design/reference/macos/playlist-player@2x.png`
/// and `player-1p-hotcue@1x.png`. The stored values above are the saturated
/// CDJ palette; the desktop draws each as a lighter, duller version, and
/// there is no formula between the two — `#00FF00` becomes `#77E866` and
/// `#0000FF` becomes `#3A59F6`, which is neither a blend with one colour nor
/// a scale of one channel. So it is a second table, measured badge by badge.
///
/// Each entry is the modal pixel of a solid fill, and each was checked in
/// more than one place: the four cues of the loaded track read the same in
/// the overview badge, the HOT CUE panel chip and the pad row, and every
/// index read the same across three rows of the track list's preview column,
/// where the 2x capture draws the badges 14 px square. The nine here are the
/// nine indices `MEASURED_CUE_COLOURS` has; the eight it lacks are unread in
/// both tables, and `cue_colour_drawn` returns `None` for them so a caller
/// falls back to one colour rather than painting a neighbour's.
pub const DRAWN_CUE_COLOURS: &[(u8, [u8; 3])] = &[
    (1, [0x3A, 0x59, 0xF6]),
    (6, [0x6A, 0xAE, 0xEC]),
    (18, [0x51, 0xAE, 0x7B]),
    (21, [0x77, 0xE8, 0x66]),
    (25, [0xA8, 0xD5, 0x4B]),
    (33, [0xD9, 0xAC, 0x3A]),
    (36, [0xF0, 0x92, 0x35]),
    (46, [0xE1, 0x3A, 0x8A]),
    (60, [0xA2, 0x74, 0xF7]),
];

/// The RGB rekordbox paints for a `ColorTableIndex`, or `None` where it has
/// not been measured.
pub fn cue_colour_drawn(index: u8) -> Option<[u8; 3]> {
    DRAWN_CUE_COLOURS.iter().find(|&&(i, _)| i == index).map(|&(_, rgb)| rgb)
}

fn cue_entry(e: &[u8], len_entry: usize) -> CueEntry {
    let mut entry = CueEntry {
        hot_cue: be32(e, 12),
        kind: e.get(16).copied().unwrap_or(0),
        time_ms: be32(e, 20),
        loop_time_ms: be32(e, 24),
        color_id: e.get(28).copied().unwrap_or(0),
        comment: None,
        color_code: None,
        rgb: None,
    };
    // A comment is what pushes the entry past the fixed part, and the colour
    // sits after the comment rather than at a fixed offset — so an entry with
    // no room for a comment length has no colour either.
    if len_entry <= CUE_ENTRY_MIN + 3 {
        return entry;
    }
    let len_comment = be32(e, CUE_ENTRY_MIN) as usize;
    let start = CUE_ENTRY_MIN + 4;
    let Some(end) = start.checked_add(len_comment).filter(|&end| end <= len_entry) else {
        return entry;
    };
    if len_comment > 0 {
        entry.comment = Some(utf16be_to_string(e.get(start..end).unwrap_or_default()));
    }
    if let Some(colour) = e.get(end..end + 4) {
        entry.color_code = colour.first().copied();
        entry.rgb = Some([
            colour.get(1).copied().unwrap_or(0),
            colour.get(2).copied().unwrap_or(0),
            colour.get(3).copied().unwrap_or(0),
        ]);
    }
    entry
}

#[derive(Debug, Clone, Default)]
pub struct Anlz {
    /// Header bytes after the 12-byte `PMAI` frame, preserved for re-emission.
    pub header_extra: Vec<u8>,
    pub sections: Vec<Section>,
}

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([
        b.get(at).copied().unwrap_or(0),
        b.get(at + 1).copied().unwrap_or(0),
        b.get(at + 2).copied().unwrap_or(0),
        b.get(at + 3).copied().unwrap_or(0),
    ])
}

/// Parses an ANLZ file.
pub fn parse(bytes: &[u8]) -> Result<Anlz> {
    if bytes.len() < SECTION_FRAME {
        return Err(AnlzError::Truncated("header"));
    }
    if bytes.get(0..4) != Some(b"PMAI") {
        return Err(AnlzError::NotAnlz);
    }
    let len_header = be32(bytes, 4) as usize;
    if len_header < SECTION_FRAME || len_header > bytes.len() {
        return Err(AnlzError::Truncated("header length"));
    }
    let header_extra = bytes.get(SECTION_FRAME..len_header).unwrap_or(&[]).to_vec();

    let mut sections = Vec::new();
    let mut at = len_header;
    while at + SECTION_FRAME <= bytes.len() {
        let tag = FourCc([
            bytes.get(at).copied().unwrap_or(0),
            bytes.get(at + 1).copied().unwrap_or(0),
            bytes.get(at + 2).copied().unwrap_or(0),
            bytes.get(at + 3).copied().unwrap_or(0),
        ]);
        let section_header = be32(bytes, at + 4) as usize;
        let section_len = be32(bytes, at + 8) as usize;

        // A zero or overlong length would loop forever or read past the end.
        if section_len < SECTION_FRAME || at + section_len > bytes.len() {
            return Err(AnlzError::BadSectionLength {
                tag: tag.to_string(),
                declared: section_len as u64,
                available: (bytes.len() - at) as u64,
            });
        }
        let header_end = (at + section_header.max(SECTION_FRAME)).min(at + section_len);
        sections.push(Section {
            tag,
            header: bytes.get(at + SECTION_FRAME..header_end).unwrap_or(&[]).to_vec(),
            payload: bytes.get(header_end..at + section_len).unwrap_or(&[]).to_vec(),
        });
        at += section_len;
    }

    Ok(Anlz { header_extra, sections })
}

fn utf16be_to_string(raw: &[u8]) -> String {
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

impl Anlz {
    pub fn read(path: &Path) -> Result<Self> {
        parse(&std::fs::read(path)?)
    }

    pub fn section(&self, tag: &[u8; 4]) -> Option<&Section> {
        let want = FourCc::new(tag);
        self.sections.iter().find(|s| s.tag == want)
    }

    pub fn beat_grid(&self) -> Option<Vec<Beat>> {
        self.sections.iter().find_map(Section::as_beat_grid)
    }

    /// Every extended cue entry in the file, across every `PCO2` section.
    pub fn cue_entries(&self) -> Vec<CueEntry> {
        self.sections.iter().filter_map(Section::as_cue_entries).flatten().collect()
    }

    pub fn path(&self) -> Option<String> {
        self.sections.iter().find_map(Section::as_path)
    }

    /// Waveform payload and its stride for a given tag.
    pub fn waveform(&self, tag: &[u8; 4]) -> Option<(u32, &[u8])> {
        let section = self.section(tag)?;
        Some((section.waveform_stride()?, section.payload.as_slice()))
    }

    /// Re-emits the file exactly as parsed.
    pub fn to_bytes(&self) -> Vec<u8> {
        write::render(&self.header_extra, &self.sections)
    }

    /// Whether the file carries `PQT2`, the extended beat grid.
    ///
    /// 112 of the first 120 `.EXT` files in the reference library have one
    /// [OBS]; the eight that do not also lack `PSSI`. Its 44-byte header
    /// decodes as: two reserved words, the constant `0x0100_0002`, then
    /// `(first beat number << 16) | tempo`, the first beat's time in
    /// milliseconds, `(last beat number << 16) | tempo`, the last beat's
    /// time, and the beat count — each confirmed against the `PQTZ` grid in
    /// the sibling `.DAT` [OBS]. Its payload is one big-endian `u16` per beat
    /// whose meaning is **[UNKNOWN]**: the values fall by roughly 84 per beat
    /// modulo about a thousand, which looks like a phase but does not divide
    /// evenly into the beat interval.
    ///
    /// That last unknown is why nothing here writes a `PQT2`. A grid edit
    /// invalidates it, and inventing a payload would put a guess into the
    /// user's library.
    #[must_use]
    pub fn has_extended_grid(&self) -> bool {
        self.section(b"PQT2").is_some()
    }

    /// The file with its beat grid replaced and every other section
    /// byte-for-byte as it was.
    ///
    /// A file with no `PQTZ` gains one, placed first — which is where every
    /// real `.DAT` carries it, after `PPTH` [OBS].
    #[must_use]
    pub fn with_beat_grid(&self, beats: &[Beat]) -> Vec<u8> {
        let replacement = write::beat_grid_section(beats);
        let mut sections = self.sections.clone();
        if let Some(at) = sections.iter().position(|s| s.tag == FourCc::new(b"PQTZ")) {
            sections[at] = replacement;
        } else {
            let after_path =
                usize::from(sections.first().is_some_and(|s| s.tag == FourCc::new(b"PPTH")));
            sections.insert(after_path, replacement);
        }
        write::render(&self.header_extra, &sections)
    }
}

/// Resolves `djmdContent.AnalysisDataPath` against the share root.
pub fn resolve(share_root: &Path, analysis_data_path: &str) -> PathBuf {
    share_root.join(analysis_data_path.trim_start_matches('/'))
}

/// The `.EXT` / `.2EX` sibling of a `.DAT` path.
pub fn sibling(dat: &Path, extension: &str) -> PathBuf {
    dat.with_extension(extension)
}
