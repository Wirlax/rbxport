//! Reader for rekordbox ANLZ analysis files (`.DAT`, `.EXT`, `.2EX`).
//!
//! An ANLZ file is a `PMAI` header followed by type-tagged sections. We parse
//! the container generically and decode the tags the application needs; any
//! unrecognised tag is kept as raw bytes rather than being dropped, so a file
//! can be re-serialised faithfully later and an unknown future tag never turns
//! into a parse failure in front of the user.
//!
//! Layouts follow the community Kaitai spec (`rekordbox_anlz.ksy`) and are
//! checked against real files from the installed library.

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

/// A cue or loop point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cue {
    /// 0 = memory cue; hot cues are 1,2,3,5,6,7,8,9 for A..H (4 is skipped).
    pub kind: u32,
    pub time_ms: u32,
    /// Loop end, or `None` when this is a plain cue.
    pub loop_end_ms: Option<u32>,
}

/// A decoded section. Unknown tags keep their bytes.
#[derive(Debug, Clone)]
pub enum Section {
    /// `PPTH` — the path of the audio file this analysis belongs to.
    Path(String),
    /// `PQTZ` — the beat grid.
    BeatGrid(Vec<Beat>),
    /// `PCOB` / `PCO2` — cue lists.
    Cues { extended: bool, cues: Vec<Cue> },
    /// `PWAV`, `PWV2`, `PWV3`… — waveform data, with its per-column stride.
    Waveform { tag: FourCc, entry_bytes: u32, data: Vec<u8> },
    /// Anything we do not decode yet, kept verbatim.
    Raw { tag: FourCc, body: Vec<u8> },
}

impl Section {
    pub fn tag(&self) -> FourCc {
        match self {
            Section::Path(_) => FourCc::new(b"PPTH"),
            Section::BeatGrid(_) => FourCc::new(b"PQTZ"),
            Section::Cues { extended, .. } => {
                if *extended { FourCc::new(b"PCO2") } else { FourCc::new(b"PCOB") }
            }
            Section::Waveform { tag, .. } | Section::Raw { tag, .. } => *tag,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Anlz {
    pub sections: Vec<Section>,
}

/// Big-endian cursor that never panics: every read is bounds-checked.
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }
    fn u2(&mut self) -> Option<u16> {
        let b = self.take(2)?;
        Some(u16::from_be_bytes([*b.first()?, *b.get(1)?]))
    }
    fn u4(&mut self) -> Option<u32> {
        let b = self.take(4)?;
        Some(u32::from_be_bytes([*b.first()?, *b.get(1)?, *b.get(2)?, *b.get(3)?]))
    }
    fn fourcc(&mut self) -> Option<FourCc> {
        let b = self.take(4)?;
        Some(FourCc([*b.first()?, *b.get(1)?, *b.get(2)?, *b.get(3)?]))
    }
}

/// Parses an ANLZ file from bytes.
pub fn parse(bytes: &[u8]) -> Result<Anlz> {
    let mut cur = Cursor::new(bytes);
    let magic = cur.fourcc().ok_or(AnlzError::Truncated("header magic"))?;
    if magic != FourCc::new(b"PMAI") {
        return Err(AnlzError::NotAnlz);
    }
    let len_header = cur.u4().ok_or(AnlzError::Truncated("header length"))?;
    let _len_file = cur.u4().ok_or(AnlzError::Truncated("file length"))?;

    // Skip to the end of the header; some versions carry extra fields.
    let header_end = usize::try_from(len_header).unwrap_or(usize::MAX);
    cur.pos = cur.pos.max(header_end.min(bytes.len()));

    let mut sections = Vec::new();
    while cur.remaining() >= 12 {
        let start = cur.pos;
        let Some(tag) = cur.fourcc() else { break };
        let Some(len_section_header) = cur.u4() else { break };
        let Some(len_tag) = cur.u4() else { break };

        // A zero or absurd length would loop forever; stop and keep what we have.
        let len_tag_usize = usize::try_from(len_tag).unwrap_or(0);
        if len_tag_usize < 12 || start.saturating_add(len_tag_usize) > bytes.len() {
            return Err(AnlzError::BadSectionLength {
                tag: tag.to_string(),
                declared: u64::from(len_tag),
                available: (bytes.len() - start) as u64,
            });
        }

        let body_start = start + usize::try_from(len_section_header).unwrap_or(12).max(12);
        let body_end = start + len_tag_usize;
        let body = bytes.get(body_start..body_end).unwrap_or(&[]);
        sections.push(decode_section(tag, body));
        cur.pos = body_end;
    }

    Ok(Anlz { sections })
}

fn decode_section(tag: FourCc, body: &[u8]) -> Section {
    let mut cur = Cursor::new(body);
    match &tag.0 {
        b"PPTH" => {
            // u4 length, then UTF-16BE, NUL-terminated.
            let len = cur.u4().unwrap_or(0) as usize;
            let raw = cur.take(len.min(cur.remaining())).unwrap_or(&[]);
            Section::Path(utf16be_to_string(raw))
        }
        b"PQTZ" => {
            let _unknown = cur.u4();
            let _always_0x80000 = cur.u4();
            let count = cur.u4().unwrap_or(0) as usize;
            let mut beats = Vec::with_capacity(count.min(cur.remaining() / 8));
            for _ in 0..count {
                let (Some(beat_number), Some(tempo_x100), Some(time_ms)) =
                    (cur.u2(), cur.u2(), cur.u4())
                else {
                    break; // truncated grid: keep the beats we did read
                };
                beats.push(Beat { beat_number, tempo_x100, time_ms });
            }
            Section::BeatGrid(beats)
        }
        b"PCOB" | b"PCO2" => {
            let extended = &tag.0 == b"PCO2";
            Section::Cues { extended, cues: decode_cues(body, extended) }
        }
        b"PWAV" | b"PWV2" => {
            let len = cur.u4().unwrap_or(0) as usize;
            let _flags = cur.u4();
            let data = cur.take(len.min(cur.remaining())).unwrap_or(&[]).to_vec();
            Section::Waveform { tag, entry_bytes: 1, data }
        }
        b"PWV3" | b"PWV4" | b"PWV5" | b"PWV6" | b"PWV7" => {
            let entry_bytes = cur.u4().unwrap_or(1);
            let entries = cur.u4().unwrap_or(0) as usize;
            let _unknown = cur.u4();
            let want = entries.saturating_mul(entry_bytes.max(1) as usize);
            let data = cur.take(want.min(cur.remaining())).unwrap_or(&[]).to_vec();
            Section::Waveform { tag, entry_bytes, data }
        }
        _ => Section::Raw { tag, body: body.to_vec() },
    }
}

/// Cue entries are themselves `PCPT`/`PCP2` sub-sections.
fn decode_cues(body: &[u8], extended: bool) -> Vec<Cue> {
    let mut cur = Cursor::new(body);
    let _list_type = cur.u4();
    let _unknown = cur.u2();
    let count = u32::from(cur.u2().unwrap_or(0));
    let mut cues = Vec::with_capacity(count as usize);
    if !extended {
        let _memory_count = cur.u4();
    }

    while cur.remaining() >= 12 {
        let start = cur.pos;
        let Some(_entry_tag) = cur.fourcc() else { break };
        let Some(_len_header) = cur.u4() else { break };
        let Some(len_entry) = cur.u4() else { break };
        let len = usize::try_from(len_entry).unwrap_or(0);
        if len < 12 || start.saturating_add(len) > body.len() {
            break;
        }

        // Layout after the sub-header: hot-cue number, status, then times.
        let hot_cue = cur.u4().unwrap_or(0);
        let _status = cur.u4();
        let _unknown = cur.u4();
        let _order = cur.u4();
        let kind = cur.u4().unwrap_or(0); // 1 = cue, 2 = loop
        let _unknown2 = cur.u4();
        let time_ms = cur.u4().unwrap_or(0);
        let loop_end = cur.u4().unwrap_or(0);

        cues.push(Cue {
            kind: hot_cue,
            time_ms,
            // 0xFFFF_FFFF marks "not a loop".
            loop_end_ms: (kind == 2 && loop_end != u32::MAX).then_some(loop_end),
        });
        cur.pos = start + len;
    }
    cues
}

fn utf16be_to_string(raw: &[u8]) -> String {
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c.first().copied().unwrap_or(0), c.get(1).copied().unwrap_or(0)]))
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
        self.sections.iter().find(|s| s.tag() == want)
    }

    pub fn beat_grid(&self) -> Option<&[Beat]> {
        self.sections.iter().find_map(|s| match s {
            Section::BeatGrid(beats) => Some(beats.as_slice()),
            _ => None,
        })
    }

    pub fn path(&self) -> Option<&str> {
        self.sections.iter().find_map(|s| match s {
            Section::Path(p) => Some(p.as_str()),
            _ => None,
        })
    }

    pub fn waveform(&self, tag: &[u8; 4]) -> Option<(&u32, &[u8])> {
        let want = FourCc::new(tag);
        self.sections.iter().find_map(|s| match s {
            Section::Waveform { tag, entry_bytes, data } if *tag == want => {
                Some((entry_bytes, data.as_slice()))
            }
            _ => None,
        })
    }
}

/// Resolves `djmdContent.AnalysisDataPath` against the share root.
///
/// The stored path is share-relative and always names the `.DAT`; the `.EXT`
/// and `.2EX` siblings carry the higher-resolution data.
pub fn resolve(share_root: &Path, analysis_data_path: &str) -> PathBuf {
    let relative = analysis_data_path.trim_start_matches('/');
    share_root.join(relative)
}

/// The `.EXT` / `.2EX` sibling of a `.DAT` path.
pub fn sibling(dat: &Path, extension: &str) -> PathBuf {
    dat.with_extension(extension)
}
