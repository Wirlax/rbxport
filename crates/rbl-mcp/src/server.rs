//! The tools. Reads come from a fresh snapshot of the database; the one
//! write goes through the app (see `app.rs`), after the same plan has been
//! checked here so a refusal never launches anything.

use std::sync::Arc;

use rbl_db::LibraryLocation;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::app::{App, Destination};
use crate::library::{Search, Snapshot, Target};

const INSTRUCTIONS: &str = "\
This server works on Ronan's rekordbox library on this Mac, through rbxport. Ronan is a DJ. He prepares \
sets as mini-sets: blocks of 2 to 4 tracks that mix into one another, each block played live as a whole, \
blocks in any order. In a playlist each block follows a separator track titled SEPARATORBREMSEN (the first \
block), then SEPARATORBREMSEN 100, 099… counting down; a separator with nothing after it is a slot kept for \
a later block. The server places the separators: never put one in a block.

Building mini-sets with him:
1. Learn his taste first: call get_mini_sets on his playlists that hold mini-sets (list_playlists says which) \
and look at what he chains.
2. Use only tracks already in his library. When he names tracks, find them with search_tracks; when a name \
matches several, ask which.
3. When he gives an idea rather than tracks, draw from the genre playlist that fits it (list_playlists: Drum \
and Bass, DNB Curated, Bass House…) unless he says otherwise. compatible_tracks finds harmonic and tempo \
neighbours. Genres in the tags are unreliable; the playlists say more.
4. Within a block keep the tempo together and prefer Camelot-compatible keys (same, relative, one step). Key \
is a guide, not a rule: a clash is fine when the tracks belong together, but say so.
5. Order inside a block is the mix order; the order of blocks does not matter.
6. Propose the blocks in plain text (title, artist, BPM, key) and wait for his explicit go-ahead. Then call \
preview_mini_sets, then add_mini_sets with exactly the approved blocks. Never write blocks he has not \
approved.
7. Writing needs rekordbox (and its agent) closed; the server starts rbxport itself when it is not open. If \
a write is refused, tell him why.";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Words to look for in the title, artist, remixer, album, label or comment. Case, accents and
    /// apostrophes are ignored and every word must match. Leave out to filter only.
    pub query: Option<String>,
    /// Only tracks in this playlist: its id, or its name.
    pub playlist: Option<String>,
    pub bpm_min: Option<f64>,
    pub bpm_max: Option<f64>,
    /// Only tracks in this key, as rekordbox writes it (`Ebm`, `F#`) or in Camelot (`2A`).
    pub key: Option<String>,
    /// At most this many tracks: 40 when left out, 200 at most.
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlaylistParams {
    /// The playlist: its id, or its name.
    pub playlist: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CompatibleParams {
    /// The track to mix from, by id.
    pub track_id: String,
    /// Only look in this playlist (its id or name), e.g. the genre playlist the set draws from.
    pub playlist: Option<String>,
    /// How far the tempo may be, in BPM: 3 when left out.
    pub bpm_tolerance: Option<f64>,
    /// At most this many tracks: 30 when left out, 200 at most.
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BlocksParams {
    /// The playlist to add the blocks to: its id or its name. Give this or `new_playlist`.
    pub playlist: Option<String>,
    /// The name of a new playlist to make at the top of the tree for just these blocks. Give this or
    /// `playlist`.
    pub new_playlist: Option<String>,
    /// The blocks in order, each a list of track ids in mix order. No separators: the server adds them.
    pub blocks: Vec<Vec<String>>,
}

impl BlocksParams {
    fn split(self) -> Result<(Target, Vec<Vec<String>>), String> {
        let target = match (self.playlist, self.new_playlist) {
            (Some(playlist), None) => Target::Playlist(playlist),
            (None, Some(name)) => Target::New(name),
            _ => return Err("Give either `playlist` or `new_playlist`, not both.".to_owned()),
        };
        Ok((target, self.blocks))
    }
}

#[derive(Clone)]
pub struct Server {
    tool_router: ToolRouter<Self>,
    app: Arc<App>,
    /// The library to read: `None` for the one rbxport would open.
    library: Option<LibraryLocation>,
}

impl Server {
    pub fn new(app: Arc<App>, library: Option<LibraryLocation>) -> Self {
        Self { tool_router: Self::tool_router(), app, library }
    }

    /// `read` on a fresh snapshot, off the async threads, as JSON.
    async fn read<T, F>(&self, read: F) -> Result<String, String>
    where
        T: Serialize,
        F: FnOnce(&Snapshot) -> Result<T, String> + Send + 'static,
    {
        let library = self.library.clone();
        tokio::task::spawn_blocking(move || {
            let snapshot = Snapshot::load(library.as_ref())?;
            let value = read(&snapshot)?;
            serde_json::to_string(&value).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

#[tool_router]
impl Server {
    /// Every playlist and folder in the library.
    #[tool(
        description = "Every playlist and folder in Ronan's rekordbox library, in tree order, with its id, path and \
                       track count. Playlists laid out in mini-sets say how many blocks and empty slots they hold.",
        annotations(read_only_hint = true)
    )]
    async fn list_playlists(&self) -> Result<String, String> {
        self.read(|snapshot| Ok(snapshot.playlists())).await
    }

    /// Tracks by words, playlist, tempo or key.
    #[tool(
        description = "Find tracks in the library by words (title, artist, remixer, album, label, comment), playlist, \
                       BPM range or key. Separator tracks are left out. Each track gives its id (what the other \
                       tools take), BPM, key with its Camelot code, length and the playlists it is in.",
        annotations(read_only_hint = true)
    )]
    async fn search_tracks(&self, Parameters(params): Parameters<SearchParams>) -> Result<String, String> {
        let search = Search {
            text: params.query,
            playlist: params.playlist,
            bpm_min: params.bpm_min,
            bpm_max: params.bpm_max,
            key: params.key,
            limit: params.limit.unwrap_or(40).clamp(1, 200),
        };
        self.read(move |snapshot| {
            let tracks = snapshot.search(&search)?;
            Ok(json!({ "count": tracks.len(), "tracks": tracks }))
        })
        .await
    }

    /// Tracks that mix with one.
    #[tool(
        description = "Tracks that mix with a given one: a Camelot-compatible key (same, relative, or one step round \
                       the wheel) and a tempo within the tolerance, smoothest first. Can be kept to one playlist, \
                       e.g. the genre playlist a set draws from.",
        annotations(read_only_hint = true)
    )]
    async fn compatible_tracks(&self, Parameters(params): Parameters<CompatibleParams>) -> Result<String, String> {
        let tolerance = params.bpm_tolerance.unwrap_or(3.0).abs();
        let limit = params.limit.unwrap_or(30).clamp(1, 200);
        let (id, playlist) = (params.track_id, params.playlist);
        self.read(move |snapshot| snapshot.compatible(id.trim(), playlist.as_deref(), tolerance, limit)).await
    }

    /// A playlist read as mini-sets.
    #[tool(
        description = "A playlist read as mini-sets: each block after its separator, with its tracks and the key and \
                       tempo transitions between them, plus the empty slots kept for the next blocks. Use it on \
                       Ronan's mini-set playlists to learn what he chains.",
        annotations(read_only_hint = true)
    )]
    async fn get_mini_sets(&self, Parameters(params): Parameters<PlaylistParams>) -> Result<String, String> {
        let playlist = params.playlist;
        self.read(move |snapshot| snapshot.mini_sets(&playlist)).await
    }

    /// What `add_mini_sets` would do, without writing.
    #[tool(
        description = "What add_mini_sets would do, without writing: the separator each block goes after (an empty \
                       slot first, then new ones on the end), its tracks, their transitions, and warnings (key \
                       clash, tempo jump over 3 BPM). Refused exactly as the write would be.",
        annotations(read_only_hint = true)
    )]
    async fn preview_mini_sets(&self, Parameters(params): Parameters<BlocksParams>) -> Result<String, String> {
        let (target, blocks) = params.split()?;
        self.read(move |snapshot| snapshot.preview(&target, &blocks)).await
    }

    /// Writes approved blocks.
    #[tool(
        description = "Writes blocks into a playlist, or into a new one at the top of the tree, through rbxport, \
                       which this server starts when it is closed. Call it only with blocks Ronan has explicitly \
                       approved in this conversation, exactly as approved, after preview_mini_sets. Needs \
                       rekordbox closed. Returns where each block went and the playlist's mini-sets afterwards.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn add_mini_sets(&self, Parameters(params): Parameters<BlocksParams>) -> Result<String, String> {
        let (target, blocks) = params.split()?;
        let library = self.library.clone();
        // Everything that can refuse is asked first, so a refusal never
        // launches the app.
        let (destination, blocks) = tokio::task::spawn_blocking(move || {
            if library.as_ref().is_none_or(|l| l.is_real_install) && rbl_db::is_rekordbox_running() {
                return Err("rekordbox (or its agent) is running: ask Ronan to quit it, then try again. Nothing was \
                            written."
                    .to_owned());
            }
            let snapshot = Snapshot::load(library.as_ref())?;
            snapshot.preview(&target, &blocks)?;
            let destination = match target {
                Target::Playlist(wanted) => Destination::Playlist(snapshot.playlist(&wanted)?.id.clone()),
                Target::New(name) => Destination::New(name.trim().to_owned()),
            };
            Ok((destination, blocks))
        })
        .await
        .map_err(|e| e.to_string())??;

        let placed = self.app.place(&destination, &blocks).await?;
        self.read(move |snapshot| {
            let id = match destination {
                Destination::Playlist(id) => Some(id),
                Destination::New(name) => snapshot.top_named(&name),
            };
            let after = id.map(|id| snapshot.mini_sets(&id)).transpose()?;
            Ok(json!({ "placed_after": placed, "playlist": after }))
        })
        .await
    }
}

#[allow(clippy::unused_async_trait_impl, reason = "the macro writes the handlers async")]
#[tool_handler(router = self.tool_router)]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("rbxport", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}
