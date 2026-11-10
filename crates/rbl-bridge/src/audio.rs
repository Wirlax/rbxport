//! A track's file, from the path the library holds for it, with byte ranges.

use std::io::SeekFrom;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::api::{header_text, ApiError, AppState};

/// What a `Range` header asks for, against a file of a known size.
#[derive(Debug, PartialEq, Eq)]
enum Wanted {
    Whole,
    /// First and last byte, inclusive.
    Bytes(u64, u64),
    Unsatisfiable,
}

/// A single range is served; several, or anything not understood, is the
/// whole file, which a server may always answer.
fn parse_range(header: Option<&str>, size: u64) -> Wanted {
    let Some(spec) = header.and_then(|h| h.trim().strip_prefix("bytes=")) else { return Wanted::Whole };
    if spec.contains(',') {
        return Wanted::Whole;
    }
    let Some((from, to)) = spec.split_once('-') else { return Wanted::Whole };
    let (from, to) = (from.trim(), to.trim());
    let last = size.saturating_sub(1);
    let range = if from.is_empty() {
        // The last `n` bytes.
        match to.parse::<u64>() {
            Ok(n) if n > 0 => (size.saturating_sub(n), last),
            Ok(_) => return Wanted::Unsatisfiable,
            Err(_) => return Wanted::Whole,
        }
    } else {
        let Ok(start) = from.parse::<u64>() else { return Wanted::Whole };
        let end = if to.is_empty() {
            last
        } else {
            match to.parse::<u64>() {
                Ok(end) => end.min(last),
                Err(_) => return Wanted::Whole,
            }
        };
        (start, end)
    };
    if size == 0 || range.0 > range.1 || range.0 >= size {
        Wanted::Unsatisfiable
    } else {
        Wanted::Bytes(range.0, range.1)
    }
}

fn content_type(extension: &str) -> &'static str {
    match extension {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "aif" | "aiff" => "audio/aiff",
        "flac" => "audio/flac",
        "m4a" | "mp4" | "aac" => "audio/mp4",
        _ => "application/octet-stream",
    }
}

fn missing() -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "file_missing", "The file is not on the disk.")
}

pub async fn serve(State(state): State<Arc<AppState>>, Path(id): Path<String>, method: Method, headers: HeaderMap) -> Result<Response, ApiError> {
    let snapshot = state.store.snapshot().await;
    let entry = snapshot.track(&id).ok_or(ApiError::new(StatusCode::NOT_FOUND, "track_not_found", "No such track."))?;
    let mut file = tokio::fs::File::open(&entry.path).await.map_err(|_| missing())?;
    let meta = file.metadata().await.map_err(|_| missing())?;
    if !meta.is_file() {
        return Err(missing());
    }
    let size = meta.len();
    let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let etag = format!("\"{size:x}-{modified:x}\"");
    let extension = entry.path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();

    let mut response = Response::builder()
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_TYPE, content_type(&extension))
        .header(header::ETAG, &etag);

    if header_text(&headers, header::IF_NONE_MATCH).is_some_and(|given| given.split(',').any(|t| t.trim() == etag)) {
        return response.status(StatusCode::NOT_MODIFIED).body(Body::empty()).map_err(|_| missing());
    }
    // A range tied to another version of the file is not applied.
    let range_header = header_text(&headers, header::RANGE).filter(|_| header_text(&headers, header::IF_RANGE).is_none_or(|v| v == etag));
    let (status, start, length) = match parse_range(range_header, size) {
        Wanted::Whole => (StatusCode::OK, 0, size),
        Wanted::Bytes(start, end) => {
            response = response.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"));
            (StatusCode::PARTIAL_CONTENT, start, end - start + 1)
        }
        Wanted::Unsatisfiable => {
            let mut refused = (StatusCode::RANGE_NOT_SATISFIABLE, Body::empty()).into_response();
            refused.headers_mut().insert(header::CONTENT_RANGE, HeaderValue::from_str(&format!("bytes */{size}")).unwrap_or(HeaderValue::from_static("bytes */*")));
            return Ok(refused);
        }
    };
    response = response.status(status).header(header::CONTENT_LENGTH, length);

    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        file.seek(SeekFrom::Start(start)).await.map_err(|_| missing())?;
        Body::from_stream(ReaderStream::with_capacity(file.take(length), 64 * 1024))
    };
    response.body(body).map_err(|_| missing())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_are_read_against_the_size() {
        assert_eq!(parse_range(None, 1000), Wanted::Whole);
        assert_eq!(parse_range(Some("bytes=0-99"), 1000), Wanted::Bytes(0, 99));
        assert_eq!(parse_range(Some("bytes=900-"), 1000), Wanted::Bytes(900, 999));
        assert_eq!(parse_range(Some("bytes=900-5000"), 1000), Wanted::Bytes(900, 999));
        assert_eq!(parse_range(Some("bytes=-100"), 1000), Wanted::Bytes(900, 999));
        assert_eq!(parse_range(Some("bytes=-5000"), 1000), Wanted::Bytes(0, 999));
        assert_eq!(parse_range(Some("bytes=1000-"), 1000), Wanted::Unsatisfiable);
        assert_eq!(parse_range(Some("bytes=50-10"), 1000), Wanted::Unsatisfiable);
        assert_eq!(parse_range(Some("bytes=0-1,5-9"), 1000), Wanted::Whole);
        assert_eq!(parse_range(Some("items=0-1"), 1000), Wanted::Whole);
    }
}
