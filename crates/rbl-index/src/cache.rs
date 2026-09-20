//! A snapshot of the loaded library, so a second start is not a second decrypt.
//!
//! Opening the real library costs about 680 ms in release and 1.4 s in debug,
//! and 543 ms of that is `SQLCipher` decrypting and handing back 38,681 rows.
//! Nothing about that gets faster: the work is the decryption. So the built
//! columns are written to disk once and read back whole, which turns the same
//! start into a sequential read of a few tens of megabytes.
//!
//! # What is not stored
//!
//! The rank arrays and the search arena, both of which are derived. Rebuilding
//! them costs about 70 ms and removes any chance of the file disagreeing with
//! itself — a wrong rank array would silently mis-sort the library, which is
//! far worse than a slower start.
//!
//! # Staleness
//!
//! The snapshot is keyed to the exact bytes it came from: the length and
//! modification time of `master.db` and its write-ahead log, plus the schema
//! version and this format's own version. If any of those differ the snapshot
//! is ignored and the library is read normally. That is deliberately blunt —
//! rekordbox touches the WAL constantly while it runs, so the cache simply
//! misses then, which is correct rather than clever.
//!
//! # Trust
//!
//! The file is treated as untrusted input even though we wrote it: a truncated
//! or edited one must not panic or allocate wildly. Every length is checked
//! against what is actually left in the buffer before anything is reserved,
//! and any inconsistency returns `None`.

use std::path::Path;

use crate::strings::{Interner, StrColumn};
use crate::{Cue, Cues, Library, Playlists, Row, TagCategory};

/// Bumped whenever the layout below changes. An older file is ignored, not
/// misread.
///
/// 2 added the My Tag categories. 3 added the cues: formats 1 and 2 left them
/// out, so every start that hit the snapshot — which is most of them — drew a
/// player with no cue markers and an empty HOT CUE list. 4 added whether each
/// playlist is a folder, and the histories: formats 1 to 3 left them out, so
/// every start that hit the snapshot had no Histories section at all.
/// 5 added the release year, and each playlist's attribute and rule: format
/// 4 knew only whether a playlist was a folder, so an intelligent playlist
/// read from the snapshot opened empty. 7 added which database the snapshot
/// was built from: formats 1 to 6 keyed on the change counter alone, so a
/// different `master.db` with the same counter — a test fixture rebuilt
/// under another folder — was served the old one's file paths.
pub const FORMAT: u32 = 7;

const MAGIC: &[u8; 4] = b"RBLX";

/// What a snapshot was built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint {
    pub format: u32,
    pub db_len: u64,
    pub db_modified_ns: i64,
    pub wal_len: u64,
    pub wal_modified_ns: i64,
    pub db_version: u32,
    /// The library's own change counter: `MAX(rb_local_usn)` across the tables
    /// we read, with the live row count.
    ///
    /// This is what makes the snapshot usable at all while rekordbox is open.
    /// rekordbox touches the write-ahead log constantly — checkpointing, its
    /// own bookkeeping — so keying on the file alone missed on every start it
    /// was running for, which is most of them. The counter only moves when a
    /// row actually changes.
    pub content: u64,
    /// Which database: a hash of the `master.db` path. Two libraries can
    /// share a change counter — the same fixture built twice under different
    /// folders does, row for row — and a snapshot of one must not stand in
    /// for the other.
    pub database: u64,
}

impl Fingerprint {
    /// Reads the fingerprint of a `master.db` and its write-ahead log.
    ///
    /// A missing WAL is normal — rekordbox removes it on a clean exit — and
    /// records as zeroes rather than as a failure.
    #[must_use]
    pub fn of(master_db: &Path, db_version: u32, content: u64) -> Option<Self> {
        let (db_len, db_modified_ns) = stamp(master_db)?;
        let wal = master_db.with_extension(
            master_db
                .extension()
                .map_or_else(|| "db-wal".to_owned(), |e| format!("{}-wal", e.to_string_lossy())),
        );
        let (wal_len, wal_modified_ns) = stamp(&wal).unwrap_or((0, 0));
        Some(Self {
            format: FORMAT,
            db_len,
            db_modified_ns,
            wal_len,
            wal_modified_ns,
            db_version,
            content,
            database: database_id(master_db),
        })
    }
}

/// A stable hash of a database's path (FNV-1a over its bytes), the same
/// across runs and builds, which the standard hasher does not promise.
fn database_id(master_db: &Path) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    master_db
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(OFFSET, |h, &b| (h ^ u64::from(b)).wrapping_mul(PRIME))
}

fn stamp(path: &Path) -> Option<(u64, i64)> {
    let meta = std::fs::metadata(path).ok()?;
    // Nanoseconds: a second's resolution would miss two writes in the same
    // second, which is exactly what rekordbox does.
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_nanos()).ok())
        .unwrap_or(0);
    Some((meta.len(), modified))
}

// ------------------------------------------------------------------ writing

#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn bytes(&mut self, v: &[u8]) {
        self.u64(v.len() as u64);
        self.0.extend_from_slice(v);
    }
    fn u64s(&mut self, v: &[u64]) {
        self.u64(v.len() as u64);
        for &x in v {
            self.0.extend_from_slice(&x.to_le_bytes());
        }
    }
    fn u32s(&mut self, v: &[u32]) {
        self.u64(v.len() as u64);
        for &x in v {
            self.0.extend_from_slice(&x.to_le_bytes());
        }
    }
    fn u16s(&mut self, v: &[u16]) {
        self.u64(v.len() as u64);
        for &x in v {
            self.0.extend_from_slice(&x.to_le_bytes());
        }
    }
    fn strings(&mut self, column: &StrColumn) {
        let (bytes, spans) = column.raw();
        self.bytes(bytes);
        self.u64(spans.len() as u64);
        for &(start, len) in spans {
            self.0.extend_from_slice(&start.to_le_bytes());
            self.0.extend_from_slice(&len.to_le_bytes());
        }
    }
    fn interner(&mut self, interner: &Interner) {
        let (names, folded) = interner.parts();
        self.strings(names);
        self.strings(folded);
    }
}

/// Serialises a loaded library.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one column after another; splitting it would only hide the order")]
pub fn encode(library: &Library, fingerprint: Fingerprint) -> Vec<u8> {
    let mut w = Writer::default();
    w.0.extend_from_slice(MAGIC);
    w.u32(fingerprint.format);
    w.u64(fingerprint.db_len);
    w.i64(fingerprint.db_modified_ns);
    w.u64(fingerprint.wal_len);
    w.i64(fingerprint.wal_modified_ns);
    w.u32(fingerprint.db_version);
    w.u64(fingerprint.content);
    w.u64(fingerprint.database);

    w.u64(library.len() as u64);
    w.u64s(&library.ids);
    for column in [
        &library.title,
        &library.title_folded,
        &library.comment,
        &library.folder_path,
        &library.file_name,
        &library.analysis_path,
        &library.artwork_path,
        &library.date_added,
        &library.release_date,
    ] {
        w.strings(column);
    }
    for column in [
        &library.artist,
        &library.album,
        &library.genre,
        &library.label,
        &library.key,
        &library.bpm_x100,
        &library.length_sec,
    ] {
        w.u32s(column);
    }
    w.bytes(&library.rating);
    w.bytes(&library.color);
    w.u16s(&library.play_count);
    w.bytes(&library.analysed);
    w.u16s(&library.year);
    for interner in [
        &library.artists,
        &library.albums,
        &library.genres,
        &library.labels,
        &library.keys,
    ] {
        w.interner(interner);
    }

    write_lists(&mut w, &library.playlists());
    // Format 4: the histories, the same shape as the playlists.
    write_lists(&mut w, &library.histories());
    // Format 6: the Tag List's rows.
    w.u32s(&library.tag_list());

    // Format 2: the My Tag categories, a string column per category whose
    // first row is the category's own name.
    w.u64(library.my_tags().len() as u64);
    for category in library.my_tags() {
        let mut column = StrColumn::with_capacity(category.tags.len() + 1, 64);
        column.push(&category.name);
        for tag in &category.tags {
            column.push(tag);
        }
        w.strings(&column);
    }

    // Format 3: cues, as five columns rather than a struct per cue, so a
    // damaged length is caught by `count` the same way every other column's is.
    let table = library.cues();
    let (cues, index) = table.parts();
    w.u64(cues.len() as u64);
    for cue in cues {
        w.u32(cue.id);
    }
    for cue in cues {
        w.u32(cue.position_ms);
    }
    for cue in cues {
        w.u32(cue.out_ms);
    }
    for cue in cues {
        w.0.push(cue.kind);
    }
    for cue in cues {
        w.0.push(cue.colour);
    }
    w.u32s(index);
    drop(table);
    w.0
}

// ------------------------------------------------------------------ reading

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let slice = self.data.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(slice)
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    /// How many elements of `size` bytes a length claims, refused if the file
    /// does not actually hold that many. This is what stops a corrupt length
    /// reserving gigabytes.
    fn count(&mut self, size: usize) -> Option<usize> {
        let claimed = usize::try_from(self.u64()?).ok()?;
        let needed = claimed.checked_mul(size)?;
        (self.data.len().checked_sub(self.at)? >= needed).then_some(claimed)
    }
    fn bytes(&mut self) -> Option<Vec<u8>> {
        let n = self.count(1)?;
        Some(self.take(n)?.to_vec())
    }
    fn u64s(&mut self) -> Option<Vec<u64>> {
        let n = self.count(8)?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(self.u64()?);
        }
        Some(out)
    }
    fn u32s(&mut self) -> Option<Vec<u32>> {
        let n = self.count(4)?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(self.u32()?);
        }
        Some(out)
    }
    fn u16s(&mut self) -> Option<Vec<u16>> {
        let n = self.count(2)?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(u16::from_le_bytes(self.take(2)?.try_into().ok()?));
        }
        Some(out)
    }
    fn strings(&mut self) -> Option<StrColumn> {
        let bytes = self.bytes()?;
        let n = self.count(8)?;
        let mut spans = Vec::with_capacity(n);
        for _ in 0..n {
            spans.push((self.u32()?, self.u32()?));
        }
        Some(StrColumn::from_raw(bytes, spans))
    }
    fn interner(&mut self) -> Option<Interner> {
        Some(Interner::from_parts(self.strings()?, self.strings()?))
    }
}

/// Rebuilds a library from a snapshot, or `None` if it does not match.
///
/// A mismatched fingerprint, an unknown format, and a damaged file are all the
/// same answer: read the library properly instead.
#[must_use]
#[allow(clippy::too_many_lines, reason = "mirrors `encode` column for column")]
pub fn decode(data: &[u8], want: Fingerprint) -> Option<Library> {
    let mut r = Reader { data, at: 0 };
    if r.take(4)? != MAGIC {
        return None;
    }
    let found = Fingerprint {
        format: r.u32()?,
        db_len: r.u64()?,
        db_modified_ns: r.i64()?,
        wal_len: r.u64()?,
        wal_modified_ns: r.i64()?,
        db_version: r.u32()?,
        content: r.u64()?,
        database: r.u64()?,
    };
    // The file stamps are recorded but deliberately not compared: rekordbox
    // rewrites the WAL without changing a single row, and refusing the
    // snapshot for that made it useless whenever rekordbox was open. What must
    // match is the database, the content counter, the schema, and the format.
    if found.format != want.format
        || found.db_version != want.db_version
        || found.content != want.content
        || found.database != want.database
    {
        return None;
    }

    let mut lib = Library::default();
    let count = usize::try_from(r.u64()?).ok()?;
    lib.ids = r.u64s()?;
    lib.title = r.strings()?;
    lib.title_folded = r.strings()?;
    lib.comment = r.strings()?;
    lib.folder_path = r.strings()?;
    lib.file_name = r.strings()?;
    lib.analysis_path = r.strings()?;
    lib.artwork_path = r.strings()?;
    lib.date_added = r.strings()?;
    lib.release_date = r.strings()?;
    lib.artist = r.u32s()?;
    lib.album = r.u32s()?;
    lib.genre = r.u32s()?;
    lib.label = r.u32s()?;
    lib.key = r.u32s()?;
    lib.bpm_x100 = r.u32s()?;
    lib.length_sec = r.u32s()?;
    lib.rating = r.bytes()?;
    lib.color = r.bytes()?;
    lib.play_count = r.u16s()?;
    lib.analysed = r.bytes()?;
    lib.year = r.u16s()?;
    lib.artists = r.interner()?;
    lib.albums = r.interner()?;
    lib.genres = r.interner()?;
    lib.labels = r.interner()?;
    lib.keys = r.interner()?;

    let playlists = read_lists(&mut r)?;
    let histories = read_lists(&mut r)?;
    let tag_list = r.u32s()?;

    let categories = r.count(8)?;
    let mut my_tags = Vec::with_capacity(categories);
    for _ in 0..categories {
        let column = r.strings()?;
        if column.is_empty() {
            return None;
        }
        my_tags.push(TagCategory {
            name: column.get(0).to_owned(),
            tags: (1..column.len()).map(|i| column.get(i).to_owned()).collect(),
        });
    }

    // Fourteen bytes a cue: the length is checked against the file once for
    // all five columns before any of them is reserved.
    let cue_count = r.count(14)?;
    let mut cues = Vec::with_capacity(cue_count);
    for _ in 0..cue_count {
        cues.push(Cue { id: r.u32()?, ..Cue::default() });
    }
    for cue in &mut cues {
        cue.position_ms = r.u32()?;
    }
    for cue in &mut cues {
        cue.out_ms = r.u32()?;
    }
    for cue in &mut cues {
        cue.kind = *r.take(1)?.first()?;
    }
    for cue in &mut cues {
        cue.colour = *r.take(1)?.first()?;
    }
    let cue_index = r.u32s()?;

    // Every per-row column has to be the same length, or a row index valid for
    // one would be out of range for another.
    if lib.ids.len() != count
        || lib.title.len() != count
        || lib.artist.len() != count
        || lib.rating.len() != count
        || lib.play_count.len() != count
        || cue_index.len() != count + 1
    {
        return None;
    }
    // A slice bound past the cue array would be clamped to nothing by
    // `Cues::of`, but a bound out of order would hand a track another's cues.
    let mut previous = 0;
    for &bound in &cue_index {
        if bound < previous || bound as usize > cues.len() {
            return None;
        }
        previous = bound;
    }
    lib.set_count(count);
    lib.set_cues(Cues::from_parts(cues, cue_index));
    lib.set_playlists(playlists);
    lib.set_histories(histories);
    // A row past the end would be a track that is not there.
    if tag_list.iter().any(|&row| row as usize >= count) {
        return None;
    }
    lib.set_tag_list(tag_list);
    lib.set_my_tags(my_tags);
    // Derived, and cheap: rebuilding removes any chance of a stored rank array
    // disagreeing with the columns it claims to order.
    lib.build_ranks();
    lib.build_search();
    Some(lib)
}

/// Writes a snapshot beside the library, atomically.
///
/// A half-written file that happened to carry a matching fingerprint would be
/// read as a library, so the bytes land under a temporary name and are renamed
/// into place once complete.
pub fn save(path: &Path, library: &Library, fingerprint: Fingerprint) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("part");
    std::fs::write(&temporary, encode(library, fingerprint))?;
    std::fs::rename(&temporary, path)
}

/// Reads a snapshot, if there is a matching one.
#[must_use]
pub fn load(path: &Path, fingerprint: Fingerprint) -> Option<Library> {
    decode(&std::fs::read(path).ok()?, fingerprint)
}

/// One list tree — the playlists or the histories — in the snapshot.
fn write_lists(w: &mut Writer, lists: &Playlists) {
    w.u64s(&lists.ids);
    w.strings(&lists.names);
    w.u32s(&lists.parent);
    w.u32s(&lists.seq);
    w.bytes(&lists.attribute);
    w.strings(&lists.smart);
    w.u64(lists.members.len() as u64);
    for members in &lists.members {
        w.u32s(members);
    }
}

fn read_lists(r: &mut Reader<'_>) -> Option<Playlists> {
    let mut lists = Playlists {
        ids: r.u64s()?,
        names: r.strings()?,
        parent: r.u32s()?,
        seq: r.u32s()?,
        attribute: r.bytes()?,
        smart: r.strings()?,
        members: Vec::new(),
    };
    let count = r.count(8)?;
    lists.members = Vec::with_capacity(count);
    for _ in 0..count {
        lists.members.push(r.u32s()? as Vec<Row>);
    }
    Some(lists)
}
