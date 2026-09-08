//! The `rbl://` scheme, which serves artwork and audio straight to the webview.
//!
//! Artwork does not go through `invoke`: a JPEG is tens of kilobytes, the IPC
//! cap is 64 KB, and base64 in a JSON response would cost a main-thread decode
//! per row. An `<img src>` lets the webview fetch, decode and cache it off the
//! UI thread, which is the whole point.
//!
//! # Why it takes a track id and not a path
//!
//! The webview names a **track**, and the path is looked up in the index. A
//! scheme that accepted a path would hand anything running in the webview a
//! read of any file the app can reach. The id is resolved against the loaded
//! library, and the resulting path is checked to still sit under the share
//! root — belt and braces, because `ImagePath` comes from the database rather
//! than from us.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::http::{Request, Response, StatusCode};

use crate::state::AppState;

/// Artwork, by track id.
const ARTWORK_HOST: &str = "artwork";
/// Audio, by track id. Answers range requests, so the player can seek without
/// pulling a whole file down first.
const AUDIO_HOST: &str = "audio";

/// The most audio to answer in one range. Big enough that playback is smooth,
/// small enough that a seek does not read tens of megabytes to discard them.
const AUDIO_CHUNK: u64 = 1024 * 1024;

/// Refuses anything larger. Real artwork is tens of kilobytes; a file this big
/// is not album art and should not be read into memory to find out.
const MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Answers one `rbl://` request.
pub fn handle(state: &Arc<AppState>, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let uri = request.uri();
    let host = uri.host();
    if host != Some(ARTWORK_HOST) && host != Some(AUDIO_HOST) {
        return status(StatusCode::NOT_FOUND);
    }
    // rbl://<host>/<track id>
    let Some(track_id) = uri.path().trim_start_matches('/').split('/').next() else {
        return status(StatusCode::BAD_REQUEST);
    };
    if track_id.is_empty() {
        return status(StatusCode::BAD_REQUEST);
    }

    let Ok(library) = state.library() else {
        return status(StatusCode::SERVICE_UNAVAILABLE);
    };

    if host == Some(AUDIO_HOST) {
        // The audio path is absolute in the database — the file's own
        // location, not a share-relative one — so it is not joined onto the
        // share root. It is still only ever resolved from a track id.
        let Some(path) = library.audio_path_of(track_id).map(PathBuf::from) else {
            return status(StatusCode::NOT_FOUND);
        };
        return serve_audio(&path, request);
    }
    let Some(relative) = library.artwork_path_of(track_id) else {
        return status(StatusCode::NOT_FOUND);
    };
    if relative.is_empty() {
        return status(StatusCode::NOT_FOUND);
    }

    let share = state.share_root();
    let Some(path) = resolve_under(&share, relative) else {
        tracing::warn!(%relative, "artwork path escapes the share root; refused");
        return status(StatusCode::FORBIDDEN);
    };

    match std::fs::metadata(&path) {
        Ok(meta) if meta.len() > MAX_BYTES => return status(StatusCode::PAYLOAD_TOO_LARGE),
        Ok(_) => {}
        Err(_) => return status(StatusCode::NOT_FOUND),
    }
    let Ok(bytes) = std::fs::read(&path) else {
        return status(StatusCode::NOT_FOUND);
    };

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type(&path))
        .header("Access-Control-Allow-Origin", "*")
        // Artwork for a given track never changes without the library
        // reloading, and the frontend re-requests with a new generation then.
        .header("Cache-Control", "max-age=31536000, immutable")
        .body(bytes)
        .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR))
}

/// Joins a share-relative path onto the root, refusing anything that climbs out.
///
/// `ImagePath` comes from the database, so it is not ours to trust: a value
/// with `..` in it would otherwise read outside the library.
fn resolve_under(root: &Path, relative: &str) -> Option<PathBuf> {
    let mut out = root.to_path_buf();
    for part in relative.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return None,
            _ => out.push(part),
        }
    }
    // Resolving symlinks too: a link inside the share tree could still point
    // out of it.
    let canonical = out.canonicalize().ok()?;
    let root = root.canonicalize().ok()?;
    canonical.starts_with(&root).then_some(canonical)
}

/// Serves an audio file, honouring a range request.
///
/// Without ranges the webview pulls the whole file before it will play, and
/// seeking re-pulls it; with them a seek costs one chunk.
fn serve_audio(path: &Path, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let Ok(meta) = std::fs::metadata(path) else {
        return status(StatusCode::NOT_FOUND);
    };
    let total = meta.len();
    if total == 0 {
        return status(StatusCode::NOT_FOUND);
    }

    let range = request
        .headers()
        .get("Range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| parse_range(v, total));

    let (start, end) = range.unwrap_or((0, (AUDIO_CHUNK - 1).min(total - 1)));
    let Ok(bytes) = read_range(path, start, end) else {
        return status(StatusCode::NOT_FOUND);
    };

    Response::builder()
        .status(if range.is_some() || end + 1 < total {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header("Content-Type", audio_type(path))
        .header("Accept-Ranges", "bytes")
        .header("Content-Range", format!("bytes {start}-{end}/{total}"))
        .header("Content-Length", (end - start + 1).to_string())
        .header("Access-Control-Allow-Origin", "*")
        .body(bytes)
        .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR))
}

/// Parses `bytes=start-end`, clamped to the file. Only the single-range form,
/// which is all a media element sends.
fn parse_range(header: &str, total: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None;
    }
    let (from, to) = spec.split_once('-')?;
    let start: u64 = if from.is_empty() {
        // A suffix range: the last N bytes.
        let n: u64 = to.parse().ok()?;
        total.saturating_sub(n)
    } else {
        from.parse().ok()?
    };
    if start >= total {
        return None;
    }
    let end = if to.is_empty() || from.is_empty() {
        total - 1
    } else {
        to.parse::<u64>().ok()?.min(total - 1)
    };
    if end < start {
        return None;
    }
    // Never answer more than a chunk, however much was asked for.
    Some((start, end.min(start + AUDIO_CHUNK - 1)))
}

/// Reads one span without pulling the whole file into memory.
fn read_range(path: &Path, start: u64, end: u64) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let wanted = usize::try_from(end - start + 1).unwrap_or(0);
    let mut out = vec![0_u8; wanted];
    let mut filled = 0;
    while filled < wanted {
        match file.read(out.get_mut(filled..).unwrap_or(&mut []))? {
            0 => break,
            n => filled += n,
        }
    }
    out.truncate(filled);
    Ok(out)
}

fn audio_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("flac") => "audio/flac",
        Some("wav") => "audio/wav",
        Some("aiff" | "aif") => "audio/aiff",
        Some("m4a" | "mp4" | "aac") => "audio/mp4",
        Some("ogg" | "opus") => "audio/ogg",
        _ => "audio/mpeg",
    }
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        // rekordbox writes .jpg for everything else it caches.
        _ => "image/jpeg",
    }
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(code)
        .body(Vec::new())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_resolves_under_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("PIONEER/Artwork/abc");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("artwork.jpg"), b"x").unwrap();

        let got = resolve_under(dir.path(), "/PIONEER/Artwork/abc/artwork.jpg");
        assert!(got.is_some());
        assert!(got.unwrap().ends_with("artwork.jpg"));
    }

    #[test]
    fn a_path_that_climbs_out_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("share")).unwrap();
        for attempt in [
            "../../../etc/passwd",
            "/PIONEER/../../etc/passwd",
            "..\\..\\Windows\\System32",
        ] {
            assert!(resolve_under(&dir.path().join("share"), attempt).is_none(), "{attempt}");
        }
    }

    #[test]
    fn a_range_header_is_parsed_and_clamped() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
        // An open end runs to the end of the file, capped at a chunk.
        assert_eq!(parse_range("bytes=500-", 1000), Some((500, 999)));
        // A suffix range: the last N bytes.
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        // Past the end of the file.
        assert_eq!(parse_range("bytes=2000-", 1000), None);
        // An end before the start.
        assert_eq!(parse_range("bytes=500-100", 1000), None);
        // Only the single-range form, which is all a media element sends.
        assert_eq!(parse_range("bytes=0-10,20-30", 1000), None);
        assert_eq!(parse_range("items=0-10", 1000), None);
        assert_eq!(parse_range("nonsense", 1000), None);
    }

    #[test]
    fn a_range_never_answers_more_than_a_chunk() {
        let huge = 50 * 1024 * 1024;
        let (start, end) = parse_range("bytes=0-", huge).expect("range");
        assert_eq!(end - start + 1, AUDIO_CHUNK);
    }

    #[test]
    fn a_span_reads_back_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.mp3");
        let data: Vec<u8> = (0..5000_u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, &data).unwrap();

        assert_eq!(read_range(&path, 0, 9).unwrap(), &data[0..10]);
        assert_eq!(read_range(&path, 1000, 1999).unwrap(), &data[1000..2000]);
        // Past the end returns what is there rather than failing.
        assert_eq!(read_range(&path, 4990, 9999).unwrap(), &data[4990..]);
    }

    #[test]
    fn audio_type_follows_the_extension() {
        assert_eq!(audio_type(Path::new("a.flac")), "audio/flac");
        assert_eq!(audio_type(Path::new("a.M4A")), "audio/mp4");
        assert_eq!(audio_type(Path::new("a.aiff")), "audio/aiff");
        // rekordbox libraries are mostly mp3, and an unknown extension is
        // likelier to be one than anything else.
        assert_eq!(audio_type(Path::new("a.unknown")), "audio/mpeg");
    }

    #[test]
    fn content_type_follows_the_extension() {
        assert_eq!(content_type(Path::new("a/b.png")), "image/png");
        assert_eq!(content_type(Path::new("a/b.PNG")), "image/png");
        assert_eq!(content_type(Path::new("a/b.jpg")), "image/jpeg");
        // rekordbox writes .jpg for everything else it caches.
        assert_eq!(content_type(Path::new("a/b")), "image/jpeg");
    }
}
