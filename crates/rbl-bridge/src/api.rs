//! The routes, their JSON shapes and the key check.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use serde_json::json;

use crate::audio;
use crate::snapshot::{PlaylistJson, Row, TrackJson};
use crate::store::{iso8601, Store};

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;

pub struct AppState {
    pub store: Store,
    pub key: String,
}

pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
}

impl ApiError {
    pub const fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self { status, code, message }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": { "code": self.code, "message": self.message } }))).into_response()
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(|| async { Json(json!({ "ok": true })) }))
        .route("/v1/status", get(status))
        .route("/v1/playlists", get(playlists))
        .route("/v1/playlists/{id}", get(playlist))
        .route("/v1/tracks", get(tracks))
        .route("/v1/tracks/{id}", get(track))
        .route("/v1/tracks/{id}/audio", get(audio::serve))
        .fallback(|| async { ApiError::new(StatusCode::NOT_FOUND, "not_found", "No such route.") })
        .layer(middleware::from_fn_with_state(Arc::clone(&state), require_key))
        .with_state(state)
}

/// Compares without stopping at the first difference.
fn same_key(given: &[u8], wanted: &[u8]) -> bool {
    let mut diff = given.len() ^ wanted.len();
    for i in 0..given.len().max(wanted.len()) {
        diff |= usize::from(given.get(i).copied().unwrap_or(0) ^ wanted.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

async fn require_key(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    if request.uri().path() != "/health" {
        let given = request.headers().get("x-bridge-key").map_or(&[][..], |v| v.as_bytes());
        if !same_key(given, state.key.as_bytes()) {
            return ApiError::new(StatusCode::UNAUTHORIZED, "unauthorized", "Missing or wrong X-Bridge-Key.").into_response();
        }
    }
    next.run(request).await
}

async fn status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let snapshot = state.store.snapshot().await;
    Json(json!({
        "bridge": "ok",
        "rekordboxRunning": state.store.rekordbox_running().await,
        "libraryUpdatedAt": snapshot.stamp.updated_at().map(iso8601),
        "trackCount": snapshot.track_count(),
        "playlistCount": snapshot.playlists().len(),
    }))
}

async fn playlists(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let snapshot = state.store.snapshot().await;
    Json(json!({ "playlists": snapshot.playlists() }))
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Item<'a> {
    Track { position: usize, track: &'a TrackJson },
    Separator { position: usize, label: &'a str },
}

async fn playlist(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, ApiError> {
    let snapshot = state.store.snapshot().await;
    let not_found = || ApiError::new(StatusCode::NOT_FOUND, "playlist_not_found", "No such playlist.");
    let head: &PlaylistJson = snapshot.playlist(&id).ok_or_else(not_found)?;
    let rows = snapshot.playlist_rows(&id).ok_or_else(not_found)?;
    let duration: i64 = rows.iter().map(|r| if let Row::Track(t) = r { t.json.duration_sec } else { 0 }).sum();
    let items: Vec<Item<'_>> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| match row {
            Row::Track(t) => Item::Track { position: i + 1, track: &t.json },
            Row::Separator(label) => Item::Separator { position: i + 1, label },
        })
        .collect();
    Ok(Json(json!({
        "id": head.id,
        "name": head.name,
        "trackCount": head.track_count,
        "durationSec": duration,
        "items": items,
    })))
}

fn number(params: &HashMap<String, String>, name: &str, default: usize) -> Result<usize, ApiError> {
    match params.get(name).map(|v| v.trim()).filter(|v| !v.is_empty()) {
        None => Ok(default),
        Some(text) => text.parse().map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "bad_request", "limit and offset are whole numbers.")),
    }
}

async fn tracks(State(state): State<Arc<AppState>>, Query(params): Query<HashMap<String, String>>) -> Result<Json<serde_json::Value>, ApiError> {
    let limit = number(&params, "limit", DEFAULT_LIMIT)?.min(MAX_LIMIT);
    let offset = number(&params, "offset", 0)?;
    let snapshot = state.store.snapshot().await;
    let (total, tracks) = snapshot.search(params.get("query").map_or("", String::as_str), offset, limit);
    Ok(Json(json!({ "total": total, "tracks": tracks })))
}

async fn track(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Result<Json<TrackJson>, ApiError> {
    let snapshot = state.store.snapshot().await;
    let entry = snapshot.track(&id).ok_or(ApiError::new(StatusCode::NOT_FOUND, "track_not_found", "No such track."))?;
    Ok(Json(entry.detailed()))
}

/// A header's text, when it is text.
pub fn header_text(headers: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use rbl_db::fixture::{self, playlist_id, track_id, Shape};
    use rbl_db::write::Writer;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;

    const KEY: &str = "secret-key";
    const AUDIO_LEN: usize = 1000;

    struct Fixture {
        dir: tempfile::TempDir,
        app: Router,
    }

    /// The default fixture library with separators as tracks 30 to 33,
    /// playlist 1 holding a block and two empty slots, a few titles and
    /// cues, and track 5 pointing at a real file.
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let location = fixture::build(dir.path(), Shape::default()).unwrap();
        let audio: Vec<u8> = (0..AUDIO_LEN).map(|i| u8::try_from(i % 251).unwrap()).collect();
        let file = dir.path().join("five.mp3");
        std::fs::write(&file, audio).unwrap();
        fixture::point_at_audio(&location, 5, file.to_str().unwrap(), 200).unwrap();

        let mut writer = Writer::open(location.clone(), dir.path().join("backups")).unwrap();
        let conn = writer.library().connection();
        for (index, title) in [(30, "SEPARATORBREMSEN"), (31, "SEPARATORBREMSEN 100"), (32, "SEPARATORBREMSEN 099"), (33, "SEPARATORBREMSEN 098")] {
            conn.execute("UPDATE djmdContent SET Title = ?1 WHERE ID = ?2", (title, track_id(index))).unwrap();
        }
        conn.execute("INSERT INTO djmdKey (ID, ScaleName, Seq, created_at, updated_at) VALUES ('k1', 'Dm', 1, '2026-10-09', '2026-10-09')", ()).unwrap();
        conn.execute("INSERT INTO djmdArtist (ID, Name, created_at, updated_at) VALUES ('a1', 'Noisia', '2026-10-09', '2026-10-09')", ()).unwrap();
        for (index, title) in [(10, "Boulder"), (11, "Booyah Jale’s Edit"), (12, "Déjà Vu")] {
            conn.execute("UPDATE djmdContent SET Title = ?1, KeyID = 'k1', ArtistID = 'a1', BPM = 17400 WHERE ID = ?2", (title, track_id(index))).unwrap();
        }
        conn.execute("UPDATE djmdContent SET BitRate = 320, DateCreated = '2026-10-09' WHERE ID = ?1", [track_id(5)]).unwrap();
        for (id, track, kind, ms, out, colour, index) in [
            ("c1", 5, 1, 20_000, -1, 255, 1),
            ("c2", 5, 0, 5_000, -1, 1, 0),
            ("c3", 5, 2, 90_000, 98_000, 255, 0),
            ("c4", 6, 0, 30_000, -1, 255, 0),
        ] {
            conn.execute(
                "INSERT INTO djmdCue (ID, ContentID, InMsec, OutMsec, Kind, Color, ColorTableIndex, Comment, rb_local_deleted, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'drop', 0, '2026-10-09', '2026-10-09')",
                (id, track_id(track), ms, out, kind, colour, index),
            )
            .unwrap();
        }
        writer.set_tracks(&playlist_id(1), &[track_id(30), track_id(10), track_id(11), track_id(31), track_id(32)]).unwrap();
        drop(writer);

        let store = Store::open(location).unwrap();
        let app = router(Arc::new(AppState { store, key: KEY.to_owned() }));
        Fixture { dir, app }
    }

    async fn call(f: &Fixture, method: &str, uri: &str, headers: &[(&str, &str)]) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut request = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = f.app.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        let (parts, body) = response.into_parts();
        (parts.status, parts.headers, to_bytes(body, usize::MAX).await.unwrap().to_vec())
    }

    async fn json(f: &Fixture, uri: &str) -> Value {
        let (status, _, body) = call(f, "GET", uri, &[("x-bridge-key", KEY)]).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn the_key_guards_everything_but_health() {
        let f = fixture();
        assert_eq!(call(&f, "GET", "/health", &[]).await.0, StatusCode::OK);
        for uri in ["/v1/status", "/v1/playlists", "/v1/tracks", "/v1/nothing"] {
            assert_eq!(call(&f, "GET", uri, &[]).await.0, StatusCode::UNAUTHORIZED, "{uri}");
            assert_eq!(call(&f, "GET", uri, &[("x-bridge-key", "nope")]).await.0, StatusCode::UNAUTHORIZED, "{uri}");
            assert_eq!(call(&f, "GET", uri, &[("x-bridge-key", "secret-ke")]).await.0, StatusCode::UNAUTHORIZED, "{uri}");
        }
        let (_, _, body) = call(&f, "GET", "/v1/status", &[]).await;
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["error"]["code"], "unauthorized");
        let status = json(&f, "/v1/status").await;
        assert_eq!(status["bridge"], "ok");
        assert_eq!(status["trackCount"], 36, "40 tracks, 4 of them separators");
        assert_eq!(status["playlistCount"], 3);
        assert!(status["libraryUpdatedAt"].as_str().unwrap().ends_with('Z'));
        assert!(status["rekordboxRunning"].is_boolean());
    }

    #[tokio::test]
    async fn playlists_count_tracks_without_separators_and_sum_their_mini_sets() {
        let f = fixture();
        let listed = json(&f, "/v1/playlists").await;
        let all = listed["playlists"].as_array().unwrap();
        assert_eq!(all.len(), 3);
        let one = all.iter().find(|p| p["id"] == playlist_id(1).as_str()).unwrap();
        assert_eq!(one["kind"], "playlist");
        assert_eq!(one["parentId"], Value::Null);
        assert_eq!(one["trackCount"], 2);
        assert_eq!(one["miniSets"], serde_json::json!({ "blocks": 1, "emptySlots": 2 }));
        let two = all.iter().find(|p| p["id"] == playlist_id(2).as_str()).unwrap();
        assert_eq!(two["miniSets"], Value::Null);
    }

    #[tokio::test]
    async fn a_playlist_lists_tracks_and_separators_in_order() {
        let f = fixture();
        let detail = json(&f, &format!("/v1/playlists/{}", playlist_id(1))).await;
        assert_eq!(detail["trackCount"], 2);
        assert_eq!(detail["durationSec"], 600);
        let items = detail["items"].as_array().unwrap();
        let shape: Vec<(&str, i64)> = items.iter().map(|i| (i["kind"].as_str().unwrap(), i["position"].as_i64().unwrap())).collect();
        assert_eq!(shape, [("separator", 1), ("track", 2), ("track", 3), ("separator", 4), ("separator", 5)]);
        assert_eq!(items[0]["label"], "SEPARATORBREMSEN");
        assert_eq!(items[4]["label"], "SEPARATORBREMSEN 099");
        let track = &items[1]["track"];
        assert_eq!(track["title"], "Boulder");
        assert_eq!((track["artist"].as_str(), track["key"].as_str(), track["camelot"].as_str(), track["bpm"].as_f64()), (Some("Noisia"), Some("Dm"), Some("7A"), Some(174.0)));
        assert_eq!(track["playlistIds"][0], playlist_id(1).as_str());
        assert_eq!(track["cues"], serde_json::json!([]));

        let (status, _, body) = call(&f, "GET", "/v1/playlists/nope", &[("x-bridge-key", KEY)]).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["error"]["code"], "playlist_not_found");
    }

    #[tokio::test]
    async fn search_folds_case_and_accents_and_needs_every_word() {
        let f = fixture();
        let titles = |v: &Value| v["tracks"].as_array().unwrap().iter().map(|t| t["title"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
        assert_eq!(titles(&json(&f, "/v1/tracks?query=DEJA%20vu").await), ["Déjà Vu"]);
        assert_eq!(titles(&json(&f, "/v1/tracks?query=jales").await), ["Booyah Jale’s Edit"]);
        let by_artist = json(&f, "/v1/tracks?query=noisia%20boulder").await;
        assert_eq!(titles(&by_artist), ["Boulder"]);
        assert_eq!(json(&f, "/v1/tracks?query=noisia%20zzz").await["total"], 0);
        assert_eq!(json(&f, "/v1/tracks?query=noisia").await["total"], 3);
        assert_eq!(json(&f, "/v1/tracks?query=separatorbremsen").await["total"], 0, "separators are never tracks");

        let page = json(&f, "/v1/tracks?limit=5&offset=2").await;
        assert_eq!(page["total"], 36);
        assert_eq!(page["tracks"].as_array().unwrap().len(), 5);
        assert_eq!(json(&f, "/v1/tracks?limit=100000").await["tracks"].as_array().unwrap().len(), 36);
        assert_eq!(call(&f, "GET", "/v1/tracks?limit=abc", &[("x-bridge-key", KEY)]).await.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn a_track_carries_its_cues_and_a_preview_point() {
        let f = fixture();
        let five = json(&f, &format!("/v1/tracks/{}", track_id(5))).await;
        assert_eq!((five["bitrate"].as_i64(), five["format"].as_str(), five["dateAdded"].as_str(), five["available"].as_bool()), (Some(320), Some("mp3"), Some("2026-10-09"), Some(true)));
        assert_eq!(five["previewMs"], 20_000, "the first hot cue from 15 s on");
        let cues = five["cues"].as_array().unwrap();
        assert_eq!(cues.len(), 3);
        let find = |kind: &str, ms: i64| cues.iter().find(|c| c["kind"] == kind && c["ms"] == ms).unwrap().clone();
        let hot_a = find("hot", 20_000);
        assert_eq!((hot_a["slot"].as_str(), hot_a["comment"].as_str()), (Some("A"), Some("drop")));
        assert!(hot_a["color"].as_str().unwrap().starts_with('#'));
        assert_eq!(find("hot", 90_000)["slot"], "B");
        assert_eq!(find("hot", 90_000)["loopMs"], 8_000);
        let memory = find("memory", 5_000);
        assert_eq!((memory["slot"].clone(), memory["color"].as_str(), memory["loopMs"].clone()), (Value::Null, Some("#E33122"), Value::Null));

        let six = json(&f, &format!("/v1/tracks/{}", track_id(6))).await;
        assert_eq!(six["previewMs"], 30_000, "a memory cue when there is no hot cue");
        assert_eq!(json(&f, &format!("/v1/tracks/{}", track_id(7))).await["previewMs"], 75_000, "a quarter of 300 s");
        let listed = json(&f, "/v1/tracks?query=five").await;
        assert_eq!(listed["tracks"][0]["previewMs"], 20_000);
        assert_eq!(listed["tracks"][0]["cues"], serde_json::json!([]));
        assert_eq!(call(&f, "GET", "/v1/tracks/999999", &[("x-bridge-key", KEY)]).await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn audio_honours_byte_ranges() {
        let f = fixture();
        let uri = format!("/v1/tracks/{}/audio", track_id(5));
        let auth = [("x-bridge-key", KEY)];
        let expected: Vec<u8> = (0..AUDIO_LEN).map(|i| u8::try_from(i % 251).unwrap()).collect();

        let (status, headers, body) = call(&f, "GET", &uri, &[auth[0], ("range", "bytes=0-99")]).await;
        assert_eq!(status, StatusCode::PARTIAL_CONTENT);
        assert_eq!(body, expected[..100]);
        assert_eq!(headers["content-range"], "bytes 0-99/1000");
        assert_eq!(headers["content-length"], "100");
        assert_eq!(headers["accept-ranges"], "bytes");
        assert_eq!(headers["content-type"], "audio/mpeg");
        let etag = headers["etag"].to_str().unwrap().to_owned();

        let (status, headers, body) = call(&f, "GET", &uri, &[auth[0], ("range", "bytes=-50")]).await;
        assert_eq!((status, body), (StatusCode::PARTIAL_CONTENT, expected[950..].to_vec()));
        assert_eq!(headers["content-range"], "bytes 950-999/1000");

        let (status, headers, body) = call(&f, "GET", &uri, &auth).await;
        assert_eq!((status, body.len(), headers["content-length"].to_str().unwrap()), (StatusCode::OK, AUDIO_LEN, "1000"));

        let (status, headers, body) = call(&f, "HEAD", &uri, &auth).await;
        assert_eq!((status, body.len(), headers["content-length"].to_str().unwrap()), (StatusCode::OK, 0, "1000"));

        assert_eq!(call(&f, "GET", &uri, &[auth[0], ("range", "bytes=5000-")]).await.0, StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(call(&f, "GET", &uri, &[auth[0], ("if-none-match", &etag)]).await.0, StatusCode::NOT_MODIFIED);
        assert_eq!(call(&f, "GET", &uri, &[]).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn audio_that_is_not_on_the_disk_is_missing() {
        let f = fixture();
        let (status, _, body) = call(&f, "GET", &format!("/v1/tracks/{}/audio", track_id(6)), &[("x-bridge-key", KEY)]).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["error"]["code"], "file_missing");
        assert_eq!(json(&f, &format!("/v1/tracks/{}", track_id(6))).await["available"], false);

        std::fs::remove_file(f.dir.path().join("five.mp3")).unwrap();
        let (status, _, _) = call(&f, "GET", &format!("/v1/tracks/{}/audio", track_id(5)), &[("x-bridge-key", KEY)]).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "a file that vanished after the snapshot");
        let (status, _, body) = call(&f, "GET", "/v1/tracks/999999/audio", &[("x-bridge-key", KEY)]).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["error"]["code"], "track_not_found");
    }
}
