//! Reading a file's tags, so a track can be added to the library.
//!
//! Separate from the writer: what a file says about itself is a different
//! question from what the database should record, and keeping them apart means
//! the tag reading is testable without a database at all.

use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::ItemKey;
use lofty::probe::Probe;

/// Extensions rekordbox will play, and so the only ones worth importing.
pub const AUDIO_EXTENSIONS: &[&str] =
    &["mp3", "m4a", "aac", "flac", "wav", "aiff", "aif", "ogg", "opus"];

/// What a file says about itself.
///
/// Every field is optional because tags routinely are: an untagged file is
/// still importable, it just arrives with its filename as its title.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackTags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub label: String,
    pub comment: String,
    /// Seconds, rounded.
    pub duration_sec: u32,
    pub bitrate: u32,
    pub sample_rate: u32,
    pub year: u16,
    pub track_no: u16,
    pub file_size: u64,
    /// Bits per sample; 16 when the format does not say (an MP3), which is
    /// what rekordbox records for one.
    pub bit_depth: u8,
}

/// `djmdContent.FileType` for a file, by extension: what rekordbox writes on
/// 38,681 reference rows [OBS] — 1 on every `.mp3`, 4 on `.m4a`, 5 on `.flac`,
/// 11 on `.wav`, 12 on `.aiff`/`.aif`. Anything else is unseen and left unset.
#[must_use]
pub fn file_type(path: &Path) -> Option<i64> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "mp3" => Some(1),
        "m4a" => Some(4),
        "flac" => Some(5),
        "wav" => Some(11),
        "aiff" | "aif" => Some(12),
        _ => None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("{0} is not a file")]
    NotAFile(String),
    #[error("{extension:?} is not a format rekordbox plays")]
    UnsupportedFormat { extension: String },
    #[error("could not read {path}: {reason}")]
    Unreadable { path: String, reason: String },
}

/// Whether a path looks like something worth importing.
#[must_use]
pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.as_str()))
}

/// Reads embedded artwork separately from the fast Explorer tag probe.
/// Prefer a front cover across all tags, then the first available picture.
pub fn read_artwork(path: &Path) -> Result<Option<Vec<u8>>, ImportError> {
    let tagged = Probe::open(path)
        .map(|probe| probe.options(ParseOptions::new().read_properties(false)))
        .and_then(Probe::read)
        .map_err(|e| ImportError::Unreadable {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;
    let pictures = || tagged.tags().iter().flat_map(lofty::tag::Tag::pictures);
    Ok(pictures()
        .find(|p| p.pic_type() == lofty::picture::PictureType::CoverFront)
        .or_else(|| pictures().next())
        .map(|p| p.data().to_vec()))
}

/// Reads a file's tags.
///
/// A missing title falls back to the file's own name rather than being left
/// empty: a library row with no title is unusable, and the filename is what
/// the person actually recognises.
pub fn read_tags(path: &Path) -> Result<TrackTags, ImportError> {
    if !path.is_file() {
        return Err(ImportError::NotAFile(path.display().to_string()));
    }
    if !is_audio(path) {
        return Err(ImportError::UnsupportedFormat {
            extension: path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_owned(),
        });
    }

    // Without the cover art. Nothing here stores a picture, and a picture is
    // most of a tag by size: reading it made one probe cost 15 ms on a local
    // disk and 56 ms cold from an SD card against under a millisecond
    // without [OBS], which is the difference between an Explorer page that
    // lands and one that stalls.
    let tagged = Probe::open(path)
        .map(|probe| probe.options(ParseOptions::new().read_cover_art(false)))
        .and_then(lofty::probe::Probe::read)
        .map_err(|e| ImportError::Unreadable {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;

    let properties = tagged.properties();
    let mut tags = TrackTags {
        duration_sec: u32::try_from(properties.duration().as_secs()).unwrap_or(0),
        bitrate: properties.audio_bitrate().unwrap_or(0),
        sample_rate: properties.sample_rate().unwrap_or(0),
        file_size: std::fs::metadata(path).map_or(0, |m| m.len()),
        bit_depth: properties.bit_depth().unwrap_or(16),
        ..TrackTags::default()
    };

    // The primary tag, or the first there is: a file can carry several, and
    // taking whichever exists beats reporting nothing.
    if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
        let text = |key: &ItemKey| tag.get_string(key).unwrap_or_default().to_owned();
        tags.title = text(&ItemKey::TrackTitle);
        tags.artist = text(&ItemKey::TrackArtist);
        tags.album = text(&ItemKey::AlbumTitle);
        tags.genre = text(&ItemKey::Genre);
        tags.label = text(&ItemKey::Label);
        tags.comment = text(&ItemKey::Comment);
        tags.year = tag
            .get_string(&ItemKey::RecordingDate)
            .and_then(|v| v.get(..4).and_then(|y| y.parse().ok()))
            .or_else(|| tag.get_string(&ItemKey::Year).and_then(|v| v.parse().ok()))
            .unwrap_or(0);
        tags.track_no = tag
            .get_string(&ItemKey::TrackNumber)
            // "3/12" is a legal track number; take the part before the slash.
            .and_then(|v| v.split('/').next().and_then(|n| n.trim().parse().ok()))
            .unwrap_or(0);
    }

    if tags.title.is_empty() {
        tags.title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    Ok(tags)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn audio_files_are_recognised_by_extension() {
        for good in ["a.mp3", "a.M4A", "a.flac", "a.aiff", "a.wav"] {
            assert!(is_audio(Path::new(good)), "{good}");
        }
        for bad in ["a.txt", "a.jpg", "a", "a.mp4", "a.mp3.txt"] {
            assert!(!is_audio(Path::new(bad)), "{bad}");
        }
    }

    #[test]
    fn a_file_that_is_not_there_is_refused_before_anything_is_read() {
        let outcome = read_tags(Path::new("/no/such/file.mp3"));
        assert!(matches!(outcome, Err(ImportError::NotAFile(_))));
    }

    #[test]
    fn a_format_rekordbox_cannot_play_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(&path, b"x").unwrap();
        assert!(matches!(
            read_tags(&path),
            Err(ImportError::UnsupportedFormat { .. })
        ));
    }

    #[test]
    fn a_file_that_is_not_really_audio_fails_rather_than_panicking() {
        // The extension says mp3; the bytes do not.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lying.mp3");
        std::fs::write(&path, b"this is not an mp3").unwrap();
        assert!(matches!(read_tags(&path), Err(ImportError::Unreadable { .. })));
    }
}
