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
use crate::{Library, Playlists, Row};

/// Bumped whenever the layout below changes. An older file is ignored, not
/// misread.
pub const FORMAT: u32 = 1;

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
}

impl Fingerprint {
    /// Reads the fingerprint of a `master.db` and its write-ahead log.
    ///
    /// A missing WAL is normal — rekordbox removes it on a clean exit — and
    /// records as zeroes rather than as a failure.
    #[must_use]
    pub fn of(master_db: &Path, db_version: u32) -> Option<Self> {
        let (db_len, db_modified_ns) = stamp(master_db)?;
        let wal = master_db.with_extension(
            master_db
                .extension()
                .map_or_else(|| "db-wal".to_owned(), |e| format!("{}-wal", e.to_string_lossy())),
        );
        let (wal_len, wal_modified_ns) = stamp(&wal).unwrap_or((0, 0));
        Some(Self { format: FORMAT, db_len, db_modified_ns, wal_len, wal_modified_ns, db_version })
    }
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
    for interner in [
        &library.artists,
        &library.albums,
        &library.genres,
        &library.labels,
        &library.keys,
    ] {
        w.interner(interner);
    }

    let playlists = library.playlists();
    w.u64s(&playlists.ids);
    w.strings(&playlists.names);
    w.u32s(&playlists.parent);
    w.u32s(&playlists.seq);
    w.u64(playlists.members.len() as u64);
    for members in &playlists.members {
        w.u32s(members);
    }
    drop(playlists);
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
    };
    if found != want {
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
    lib.artists = r.interner()?;
    lib.albums = r.interner()?;
    lib.genres = r.interner()?;
    lib.labels = r.interner()?;
    lib.keys = r.interner()?;

    let mut playlists = Playlists {
        ids: r.u64s()?,
        names: r.strings()?,
        parent: r.u32s()?,
        seq: r.u32s()?,
        members: Vec::new(),
    };
    let lists = r.count(8)?;
    playlists.members = Vec::with_capacity(lists);
    for _ in 0..lists {
        playlists.members.push(r.u32s()? as Vec<Row>);
    }

    // Every per-row column has to be the same length, or a row index valid for
    // one would be out of range for another.
    if lib.ids.len() != count
        || lib.title.len() != count
        || lib.artist.len() != count
        || lib.rating.len() != count
        || lib.play_count.len() != count
    {
        return None;
    }
    lib.set_count(count);
    lib.set_playlists(playlists);
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
