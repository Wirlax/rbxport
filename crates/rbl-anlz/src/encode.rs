//! Waveform payloads from band columns, and whole analysis files from a
//! track's analysis.
//!
//! The encodings are the ones the app's own drawing reads (`src/canvas/
//! waveform.ts`) and `rbl-link` serves to players, written back the other
//! way:
//!
//! - `PWAV`, `PWV2`, `PWV3`: one byte a column, five bits of height and
//!   three of "whiteness" — how much of the column is treble.
//! - `PWV4`: six bytes a column, the first the height out of 127 and the
//!   last three the red, green and blue, which track the mid, high and low
//!   bands [OBS]. Bytes 1 and 2 are `[UNKNOWN]` and written as zero.
//! - `PWV5`: a big-endian `u16` a column, `rrrgggbbbhhhhh00`.
//! - `PWV6`, `PWV7`: three bytes a column, low, mid and high out of 127.
//!
//! `PWAV` is 400 columns and `PWV2` 100; `PWV4` and `PWV6` are 1,200; the
//! scrolling `PWV3`, `PWV5` and `PWV7` are 150 a second [OBS]. A coarser
//! waveform is the loudest column of each bucket of the fine one, which is
//! what keeps a kick visible in the overview.

use rbl_core::FourCc;

use crate::write::{beat_grid_section, AnlzBuilder};
use crate::{Anlz, Beat, Section};

/// One column of the analysed waveform, every value out of 255.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BandColumn {
    pub low: u8,
    pub mid: u8,
    pub high: u8,
    /// The column's overall peak, which is its height.
    pub peak: u8,
}

/// Columns in the overview waveforms `PWV4` and `PWV6`.
pub const OVERVIEW_COLUMNS: usize = 1200;
/// Columns in the `PWAV` preview.
pub const PREVIEW_COLUMNS: usize = 400;
/// Columns in the `PWV2` tiny preview.
pub const TINY_COLUMNS: usize = 100;

/// `n` columns from any number: the loudest column of each bucket.
#[must_use]
pub fn resample(columns: &[BandColumn], n: usize) -> Vec<BandColumn> {
    if n == 0 {
        return Vec::new();
    }
    if columns.is_empty() {
        return vec![BandColumn::default(); n];
    }
    let mut out = Vec::with_capacity(n);
    for bucket in 0..n {
        let start = bucket * columns.len() / n;
        let end = ((bucket + 1) * columns.len() / n).max(start + 1).min(columns.len());
        let mut loudest = BandColumn::default();
        for column in columns.get(start..end).unwrap_or(&[]) {
            loudest.low = loudest.low.max(column.low);
            loudest.mid = loudest.mid.max(column.mid);
            loudest.high = loudest.high.max(column.high);
            loudest.peak = loudest.peak.max(column.peak);
        }
        out.push(loudest);
    }
    out
}

/// The one-byte encoding: height in the low five bits, whiteness above.
fn mono_byte(column: BandColumn) -> u8 {
    let height = u32::from(column.peak) * 31 / 255;
    let total = u32::from(column.low) + u32::from(column.mid) + u32::from(column.high);
    let whiteness = (u32::from(column.high) * 7 + total / 2).checked_div(total).unwrap_or(0);
    u8::try_from((whiteness.min(7) << 5) | height.min(31)).unwrap_or(0)
}

/// `PWAV`: 400 columns, one byte each.
#[must_use]
pub fn pwav(columns: &[BandColumn]) -> Vec<u8> {
    resample(columns, PREVIEW_COLUMNS).into_iter().map(mono_byte).collect()
}

/// `PWV2`: 100 columns, one byte each.
#[must_use]
pub fn pwv2(columns: &[BandColumn]) -> Vec<u8> {
    resample(columns, TINY_COLUMNS).into_iter().map(mono_byte).collect()
}

/// `PWV3`: every column, one byte each.
#[must_use]
pub fn pwv3(columns: &[BandColumn]) -> Vec<u8> {
    columns.iter().copied().map(mono_byte).collect()
}

/// `PWV4`: 1,200 columns of six bytes.
#[must_use]
pub fn pwv4(columns: &[BandColumn]) -> Vec<u8> {
    let mut out = Vec::with_capacity(OVERVIEW_COLUMNS * 6);
    for column in resample(columns, OVERVIEW_COLUMNS) {
        out.extend_from_slice(&[column.peak >> 1, 0, 0, column.mid, column.high, column.low]);
    }
    out
}

/// `PWV5`: every column as a big-endian word, `rrrgggbbbhhhhh00`.
#[must_use]
pub fn pwv5(columns: &[BandColumn]) -> Vec<u8> {
    let mut out = Vec::with_capacity(columns.len() * 2);
    for column in columns {
        let word = (u16::from(column.mid >> 5) << 13)
            | (u16::from(column.high >> 5) << 10)
            | (u16::from(column.low >> 5) << 7)
            | (u16::from(column.peak >> 3) << 2);
        out.extend_from_slice(&word.to_be_bytes());
    }
    out
}

/// `PWV6`: 1,200 columns of three bytes, low, mid and high out of 127.
#[must_use]
pub fn pwv6(columns: &[BandColumn]) -> Vec<u8> {
    let mut out = Vec::with_capacity(OVERVIEW_COLUMNS * 3);
    for column in resample(columns, OVERVIEW_COLUMNS) {
        out.extend_from_slice(&[column.low >> 1, column.mid >> 1, column.high >> 1]);
    }
    out
}

/// `PWV7`: every column as three bytes.
#[must_use]
pub fn pwv7(columns: &[BandColumn]) -> Vec<u8> {
    let mut out = Vec::with_capacity(columns.len() * 3);
    for column in columns {
        out.extend_from_slice(&[column.low >> 1, column.mid >> 1, column.high >> 1]);
    }
    out
}

/// What one analysis produces on disk: the three files rekordbox keeps
/// beside each other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisFiles {
    pub dat: Vec<u8>,
    pub ext: Vec<u8>,
    pub two_ex: Vec<u8>,
}

/// The files already there for the track, whose sections this analysis
/// cannot produce are carried through.
#[derive(Debug, Clone, Copy, Default)]
pub struct Existing<'a> {
    pub dat: Option<&'a Anlz>,
    pub ext: Option<&'a Anlz>,
    pub two_ex: Option<&'a Anlz>,
}

/// The tags an analysis writes afresh, so a carried file's copies of them
/// are dropped rather than doubled.
const AUTHORED: [&[u8; 4]; 10] =
    [b"PPTH", b"PQTZ", b"PWAV", b"PWV2", b"PWV3", b"PWV4", b"PWV5", b"PWV6", b"PWV7", b"PQT2"];

/// Authors the three analysis files for a track.
///
/// `audio_path` is what `PPTH` names: the file's own path, as
/// `djmdContent.FolderPath` holds it. The grid and the waveforms are written
/// from the analysis; everything else in an existing file — the cue lists,
/// `PSSI`, `PVDI`, tags nobody has named — is carried through in its own
/// order, because rekordbox authored it and this cannot. `PQT2` is the one
/// exception: it is an extended copy of the grid, and a stale one beside a
/// new grid is worse than none, so it is dropped (nothing here can write
/// one; see [`Anlz::has_extended_grid`]).
///
/// A file with nothing to carry gets the empty cue lists the share tree
/// holds: cues live in `djmdCue`, and every share-tree cue list in the
/// reference library is empty [OBS].
#[must_use]
pub fn author(audio_path: &str, beats: &[Beat], columns: &[BandColumn], existing: Existing<'_>) -> AnalysisFiles {
    let dat = {
        let mut builder = AnlzBuilder::new();
        builder.path(audio_path);
        builder.beat_grid(beats);
        builder.waveform_preview(b"PWAV", &pwav(columns));
        builder.waveform_preview(b"PWV2", &pwv2(columns));
        carry_or(&mut builder, existing.dat, |b| {
            b.empty_cue_list_of(false, 0);
            b.empty_cue_list_of(false, 1);
        });
        builder.finish()
    };
    let ext = {
        let mut builder = AnlzBuilder::new();
        builder.path(audio_path);
        builder.waveform_scroll(b"PWV3", 1, &pwv3(columns));
        builder.waveform_scroll(b"PWV4", 6, &pwv4(columns));
        builder.waveform_scroll(b"PWV5", 2, &pwv5(columns));
        carry_or(&mut builder, existing.ext, |b| {
            b.empty_cue_list_of(true, 0);
            b.empty_cue_list_of(true, 1);
        });
        builder.finish()
    };
    let two_ex = {
        let mut builder = AnlzBuilder::new();
        builder.path(audio_path);
        builder.waveform_scroll(b"PWV6", 3, &pwv6(columns));
        builder.waveform_scroll(b"PWV7", 3, &pwv7(columns));
        carry_or(&mut builder, existing.two_ex, |_| {});
        builder.finish()
    };
    AnalysisFiles { dat, ext, two_ex }
}

/// Copies an existing file's other sections, or writes the defaults for a
/// file that has none.
fn carry_or(builder: &mut AnlzBuilder, existing: Option<&Anlz>, defaults: impl FnOnce(&mut AnlzBuilder)) {
    match existing {
        Some(file) => {
            builder.header_extra(&file.header_extra);
            for section in file.sections.iter().filter(|s| !is_authored(s)) {
                builder.copy_section(section);
            }
        }
        None => defaults(builder),
    }
}

fn is_authored(section: &Section) -> bool {
    AUTHORED.iter().any(|tag| section.tag == FourCc::new(tag))
}

/// The grid as `Beat`s from the analyser's own beat list, which shares the
/// field names but not the type.
#[must_use]
pub fn beat_grid_of(beats: impl IntoIterator<Item = (u16, u16, u32)>) -> Vec<Beat> {
    beats
        .into_iter()
        .map(|(beat_number, tempo_x100, time_ms)| Beat { beat_number, tempo_x100, time_ms })
        .collect()
}

/// A `PQTZ` on its own, for callers that replace a grid in place.
#[must_use]
pub fn grid_section(beats: &[Beat]) -> Section {
    beat_grid_section(beats)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn ramp(n: usize) -> Vec<BandColumn> {
        (0..n)
            .map(|i| {
                let v = u8::try_from(i * 255 / n.saturating_sub(1).max(1)).unwrap_or(255);
                BandColumn { low: v, mid: v / 2, high: v / 4, peak: v }
            })
            .collect()
    }

    #[test]
    fn coarser_waveforms_keep_the_loudest_column_of_each_bucket() {
        let mut columns = vec![BandColumn::default(); 3000];
        columns[1500] = BandColumn { low: 200, mid: 10, high: 10, peak: 200 };
        let overview = resample(&columns, 1200);
        assert_eq!(overview.len(), 1200);
        assert_eq!(overview[600].peak, 200, "the kick survives the resample");
        assert_eq!(resample(&[], 4), vec![BandColumn::default(); 4]);
        assert!(resample(&columns, 0).is_empty());
    }

    #[test]
    fn the_payloads_have_their_strides_and_ranges() {
        let columns = ramp(3000);
        assert_eq!(pwav(&columns).len(), 400);
        assert_eq!(pwv2(&columns).len(), 100);
        assert_eq!(pwv3(&columns).len(), 3000);
        assert_eq!(pwv4(&columns).len(), 7200);
        assert_eq!(pwv5(&columns).len(), 6000);
        assert_eq!(pwv6(&columns).len(), 3600);
        assert_eq!(pwv7(&columns).len(), 9000);
        // Heights stay inside their fields: the loudest column fills them.
        assert_eq!(pwav(&columns).iter().map(|b| b & 0x1f).max(), Some(31));
        assert!(pwv4(&columns).chunks(6).all(|c| c[0] <= 127));
        assert!(pwv6(&columns).iter().all(|&b| b <= 127));
        // A treble-only column is white; a bass-only one is not.
        assert_eq!(mono_byte(BandColumn { low: 0, mid: 0, high: 255, peak: 255 }) >> 5, 7);
        assert_eq!(mono_byte(BandColumn { low: 255, mid: 0, high: 0, peak: 255 }) >> 5, 0);
        let word = u16::from_be_bytes(pwv5(&[BandColumn { low: 255, mid: 255, high: 255, peak: 255 }])[..2].try_into().unwrap());
        assert_eq!(word, 0b1111_1111_1111_1100);
    }

    #[test]
    fn authored_files_parse_and_carry_what_they_cannot_write() {
        let beats = beat_grid_of([(1, 12_800, 0), (2, 12_800, 469), (3, 12_800, 938), (4, 12_800, 1406)]);
        let columns = ramp(600);
        let fresh = author("/Music/track.mp3", &beats, &columns, Existing::default());
        let dat = crate::parse(&fresh.dat).unwrap();
        assert_eq!(dat.path().as_deref(), Some("/Music/track.mp3"));
        assert_eq!(dat.beat_grid().unwrap(), beats);
        assert_eq!(dat.waveform(b"PWAV").unwrap().1.len(), 400);
        assert_eq!(dat.sections.iter().filter(|s| s.is_cue_list()).count(), 2);
        let ext = crate::parse(&fresh.ext).unwrap();
        assert_eq!(ext.waveform(b"PWV5").unwrap(), (2, &pwv5(&columns)[..]));
        assert_eq!(ext.waveform(b"PWV4").unwrap().0, 6);
        let two = crate::parse(&fresh.two_ex).unwrap();
        assert_eq!(two.waveform(b"PWV7").unwrap(), (3, &pwv7(&columns)[..]));

        // A second analysis carries the phrase section through and drops the
        // stale extended grid, keeping the file's own header bytes.
        let mut previous = ext.clone();
        previous.header_extra = vec![9; 16];
        previous.sections.push(Section::new(b"PSSI", vec![0; 8], vec![1, 2, 3]));
        previous.sections.push(Section::new(b"PQT2", vec![0; 32], vec![0; 8]));
        let again = author("/Music/track.mp3", &beats, &columns, Existing { ext: Some(&previous), ..Existing::default() });
        let reread = crate::parse(&again.ext).unwrap();
        assert!(reread.section(b"PSSI").is_some());
        assert!(!reread.has_extended_grid());
        assert_eq!(reread.header_extra, vec![9; 16]);
        assert_eq!(reread.sections.iter().filter(|s| s.tag == FourCc::new(b"PWV3")).count(), 1);
    }
}
