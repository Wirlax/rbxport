//! Builders for tests and fixtures. Never used against the real library.

use crate::{strings::fold, Library, Playlists, Row};

/// Minimal track description for constructing an index without a database.
#[derive(Debug, Clone, Default)]
pub struct TestTrack {
    pub id: u64,
    pub title: &'static str,
    pub artist: &'static str,
    pub album: &'static str,
    pub comment: &'static str,
    pub bpm_x100: u32,
    pub length_sec: u32,
    pub rating: u8,
    pub date_added: &'static str,
    pub key: &'static str,
    /// `ColorID`, 1 to 8; 0 is none.
    pub color: u8,
}

/// Builds an index directly, bypassing SQL.
pub fn library_from(tracks: &[TestTrack]) -> Library {
    let mut lib = Library::default();
    for t in tracks {
        lib.ids.push(t.id);
        lib.title.push(t.title);
        lib.title_folded.push(&fold(t.title));
        lib.comment.push(t.comment);
        lib.folder_path.push("");
        lib.file_name.push("");
        lib.analysis_path.push("");
        lib.date_added.push(t.date_added);
        lib.release_date.push("");
        lib.artist.push(lib.artists.push(t.artist));
        lib.album.push(lib.albums.push(t.album));
        lib.genre.push(lib.genres.push(""));
        lib.label.push(lib.labels.push(""));
        // One interner entry per track, names repeating, which is what
        // `djmdKey` does on the reference library (`A` under two ids).
        lib.key.push(lib.keys.push(t.key));
        lib.bpm_x100.push(t.bpm_x100);
        lib.length_sec.push(t.length_sec);
        lib.rating.push(t.rating);
        lib.color.push(t.color);
        lib.play_count.push(0);
        lib.analysed.push(u8::from(t.bpm_x100 > 0));
    }
    lib.count = tracks.len();
    lib.set_playlists(Playlists::default());
    lib.build_ranks();
    lib.build_search();
    lib
}

/// Adds a playlist over the given row indices, returning its index.
pub fn add_playlist(lib: &mut Library, name: &str, rows: &[Row]) -> usize {
    let mut playlists = (*lib.playlists()).clone();
    let index = playlists.ids.len();
    playlists.ids.push(1000 + u64::try_from(index).unwrap_or(0));
    playlists.names.push(name);
    playlists.parent.push(crate::NO_ID);
    playlists.seq.push(u32::try_from(index).unwrap_or(0));
    playlists.members.push(rows.to_vec());
    lib.set_playlists(playlists);
    index
}

/// Sets the My Tag categories, which a fixture without a database cannot read.
pub fn set_my_tags(lib: &mut Library, categories: Vec<crate::TagCategory>) {
    lib.set_my_tags(categories);
}
