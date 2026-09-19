//! The read-only tree a player sees, and the file handles that address it.
//!
//! The tree is built once from the export plan; a player can only reach a node
//! by walking from the mount point, one name at a time. Names are never joined
//! onto a path and re-opened, so `..`, an absolute name, or a symlink in a name
//! cannot escape the export — there is nothing to escape *to*, because the only
//! real paths in existence are the ones we registered.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A file handle is a fixed 32 opaque bytes in `NFSv2`.
pub const HANDLE_LEN: usize = 32;

/// The most a single `READ` may return: rekordbox's libFilSiNE caps a read
/// at 0xfc00 (`docs/pre-release/rekordbox/link-export-internals.md`). A
/// CDJ-3000 asks for 32 KB at a time (679 of 694 reads in the 2026-09-12
/// capture; the rest were the tail of the file), and the reply goes out as
/// one UDP datagram in IP fragments. `NFSv2`'s nominal 8 KB ceiling is not
/// what the players use.
pub const MAX_READ: usize = 0xfc00;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Directory,
    File,
}

#[derive(Debug, Clone)]
struct Node {
    name: String,
    kind: NodeKind,
    parent: usize,
    children: Vec<usize>,
    /// Where a file's bytes actually live. Directories have none.
    source: Option<PathBuf>,
    /// What the host says about the file. Given at insertion, or — for a
    /// tree built from a 38,681-track index, where a `stat` per file at
    /// start would cost seconds — read from the file the first time a player
    /// asks.
    stat: OnceLock<Stat>,
}

/// What `stat` says about a file, as the `fattr` reports it: rekordbox's
/// libFilSiNE hands a player the host's own mode, owner, block size and
/// device (`docs/pre-release/rekordbox/link-export-internals.md`), so a
/// player sees the file as the host does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stat {
    pub size: u64,
    /// `st_mode`, the type bits included.
    pub mode: u32,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub blocksize: u32,
    pub rdev: u32,
    pub blocks: u32,
    /// Seconds since the epoch.
    pub accessed: u32,
    pub modified: u32,
    pub changed: u32,
}

impl Stat {
    /// A plain readable file of `size` bytes, changed at `modified`: what a
    /// node given its size at insertion reports.
    pub fn plain(size: u64, modified: u32) -> Self {
        Self {
            size,
            mode: 0o100_444,
            nlink: 1,
            uid: 0,
            gid: 0,
            blocksize: 4096,
            rdev: 0,
            blocks: u32::try_from(size.div_ceil(512)).unwrap_or(u32::MAX),
            accessed: modified,
            modified,
            changed: modified,
        }
    }

    /// A directory of the tree. The directories here are the library's,
    /// not the host's, so they carry what rekordbox's export root showed a
    /// player in the capture: mode `041ed`, two links, 64 bytes.
    pub fn directory(modified: u32) -> Self {
        Self {
            size: 64,
            mode: 0o040_755,
            nlink: 2,
            uid: 0,
            gid: 0,
            blocksize: 4096,
            rdev: 0,
            blocks: 0,
            accessed: modified,
            modified,
            changed: modified,
        }
    }
}

fn seconds(time: std::io::Result<std::time::SystemTime>) -> u32 {
    time.ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| u32::try_from(d.as_secs()).unwrap_or(u32::MAX))
}

/// The host's `stat` of the file, or an empty plain file for one that
/// cannot be read: a missing file is listed with no size and fails on
/// `READ`, which is what a player expects of a moved track.
fn stat_file(path: &Path) -> Stat {
    let Ok(meta) = std::fs::metadata(path) else {
        return Stat::plain(0, 0);
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let low = |v: u64| u32::try_from(v).unwrap_or(u32::MAX);
        Stat {
            size: meta.len(),
            mode: meta.mode() & 0xffff,
            nlink: low(meta.nlink()),
            uid: meta.uid(),
            gid: meta.gid(),
            blocksize: low(meta.blksize()),
            rdev: low(meta.dev()),
            blocks: low(meta.blocks()),
            accessed: seconds(meta.accessed()),
            modified: seconds(meta.modified()),
            changed: u32::try_from(meta.ctime()).unwrap_or(0),
        }
    }
    #[cfg(not(unix))]
    {
        let mut stat = Stat::plain(meta.len(), seconds(meta.modified()));
        stat.accessed = seconds(meta.accessed());
        stat.changed = seconds(meta.created());
        stat
    }
}

/// An exported filesystem and everything reachable inside it.
#[derive(Debug, Clone)]
pub struct Vfs {
    /// The export name a player mounts, e.g. `/` on macOS or `/C/` on Windows.
    export: String,
    nodes: Vec<Node>,
    /// Where this export's file ids start: libFilSiNE numbers every node
    /// of every export from one table, so a handle names its export by its
    /// ids alone. Set when the export joins an [`Exports`].
    id_base: u32,
}

/// Addresses one node, laid out as rekordbox's libFilSiNE lays its handles
/// out: three big-endian file ids — the node's, its parent's, the mount
/// root's — then twenty zero bytes, the root's being its own id three times
/// (`docs/pre-release/rekordbox/link-export-internals.md`, "File handles").
/// Checked on the way back in: the three must agree with the tree, so a
/// handle from another export, or a made-up one, is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handle([u8; HANDLE_LEN]);

impl Handle {
    pub const fn as_bytes(&self) -> &[u8; HANDLE_LEN] {
        &self.0
    }

    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        let mut out = [0_u8; HANDLE_LEN];
        out.copy_from_slice(bytes.get(..HANDLE_LEN)?);
        Some(Self(out))
    }
}

/// Node attributes, as `NFSv2` reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attributes {
    pub kind: NodeKind,
    pub size: u64,
    pub fileid: u32,
    pub modified: u32,
    /// The rest of what the host says, for the `fattr`.
    pub stat: Stat,
}

impl Vfs {
    /// Creates an empty export. `export` is the name a player mounts.
    pub fn new(export: impl Into<String>) -> Self {
        let export = export.into();
        Self {
            id_base: 0,
            export,
            nodes: vec![Node {
                name: String::new(),
                kind: NodeKind::Directory,
                parent: 0,
                children: Vec::new(),
                source: None,
                stat: OnceLock::from(Stat::directory(0)),
            }],
        }
    }

    pub fn export_name(&self) -> &str {
        &self.export
    }

    pub const fn root(&self) -> usize {
        0
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        // The root always exists, so a fresh tree has exactly one node.
        self.nodes.len() <= 1
    }

    /// Adds a file at a slash-separated path inside the export, creating the
    /// directories it needs. Returns the node index.
    ///
    /// Empty and `.`/`..` components are dropped rather than honoured: a caller
    /// building a tree has no business asking for them, and silently resolving
    /// them is how an export grows a hole.
    pub fn add_file(&mut self, path: &str, source: impl Into<PathBuf>, size: u64, modified: u32) -> usize {
        let mut at = self.root();
        let parts: Vec<&str> = path
            .split('/')
            .filter(|part| !part.is_empty() && *part != "." && *part != "..")
            .collect();
        let Some((last, directories)) = parts.split_last() else {
            return at;
        };
        for part in directories {
            at = self.child_or_insert(at, part, NodeKind::Directory, None, Some(Stat::directory(modified)));
        }
        self.child_or_insert(at, last, NodeKind::File, Some(source.into()), Some(Stat::plain(size, modified)))
    }

    /// Adds a file whose size and modification time are read from `source`
    /// the first time a player asks for its attributes, not now.
    pub fn add_file_unsized(&mut self, path: &str, source: impl Into<PathBuf>) -> usize {
        let mut at = self.root();
        let parts: Vec<&str> = path
            .split('/')
            .filter(|part| !part.is_empty() && *part != "." && *part != "..")
            .collect();
        let Some((last, directories)) = parts.split_last() else {
            return at;
        };
        for part in directories {
            at = self.child_or_insert(at, part, NodeKind::Directory, None, Some(Stat::directory(0)));
        }
        self.child_or_insert(at, last, NodeKind::File, Some(source.into()), None)
    }

    /// Adds an empty directory, for a tree that must show a folder with no files.
    pub fn add_dir(&mut self, path: &str) -> usize {
        let mut at = self.root();
        for part in path.split('/').filter(|p| !p.is_empty() && *p != "." && *p != "..") {
            at = self.child_or_insert(at, part, NodeKind::Directory, None, Some(Stat::directory(0)));
        }
        at
    }

    fn child_or_insert(
        &mut self,
        parent: usize,
        name: &str,
        kind: NodeKind,
        source: Option<PathBuf>,
        stat: Option<Stat>,
    ) -> usize {
        if let Some(existing) = self.child(parent, name) {
            return existing;
        }
        let index = self.nodes.len();
        self.nodes.push(Node {
            name: name.to_owned(),
            kind,
            parent,
            children: Vec::new(),
            source,
            stat: stat.map_or_else(OnceLock::new, OnceLock::from),
        });
        if let Some(node) = self.nodes.get_mut(parent) {
            node.children.push(index);
        }
        index
    }

    /// Looks up one name in one directory. This is the only way in.
    pub fn child(&self, parent: usize, name: &str) -> Option<usize> {
        let node = self.nodes.get(parent)?;
        if node.kind != NodeKind::Directory {
            return None;
        }
        // `.` and `..` are answered here rather than by name matching, so they
        // stay inside the export: `..` at the root is the root.
        match name {
            "." => return Some(parent),
            ".." => return Some(node.parent),
            _ => {}
        }
        // A name a player read off a listing comes back decomposed (NFD),
        // the way rekordbox's `UTF8-MAC` conversion sent it; the tree holds
        // it as the library wrote it. Either form finds the child.
        let exact = node.children.iter().copied().find(|index| self.nodes.get(*index).is_some_and(|c| c.name == name));
        exact.or_else(|| {
            use unicode_normalization::UnicodeNormalization as _;
            let wanted: String = name.nfd().collect();
            node.children
                .iter()
                .copied()
                .find(|index| self.nodes.get(*index).is_some_and(|c| c.name.nfd().eq(wanted.chars())))
        })
    }

    /// The name of a node as it goes on the wire: decomposed (NFD) on Apple
    /// hosts, where rekordbox converts through `UTF8-MAC` both ways, and as
    /// it is elsewhere.
    pub fn wire_name(&self, index: usize) -> Option<String> {
        let name = self.name(index)?;
        if cfg!(target_vendor = "apple") {
            use unicode_normalization::UnicodeNormalization as _;
            Some(name.nfd().collect())
        } else {
            Some(name.to_owned())
        }
    }

    /// Walks a whole slash-separated path from the root, one name at a time.
    pub fn resolve(&self, path: &str) -> Option<usize> {
        let mut at = self.root();
        for part in path.split('/').filter(|part| !part.is_empty()) {
            at = self.child(at, part)?;
        }
        Some(at)
    }

    pub fn kind(&self, index: usize) -> Option<NodeKind> {
        self.nodes.get(index).map(|node| node.kind)
    }

    pub fn name(&self, index: usize) -> Option<&str> {
        self.nodes.get(index).map(|node| node.name.as_str())
    }

    pub fn source(&self, index: usize) -> Option<&Path> {
        self.nodes.get(index)?.source.as_deref()
    }

    pub fn children(&self, index: usize) -> &[usize] {
        self.nodes.get(index).map_or(&[], |node| node.children.as_slice())
    }

    pub fn attributes(&self, index: usize) -> Option<Attributes> {
        let node = self.nodes.get(index)?;
        let stat = *node
            .stat
            .get_or_init(|| node.source.as_deref().map_or_else(|| Stat::plain(0, 0), stat_file));
        Some(Attributes {
            kind: node.kind,
            size: stat.size,
            fileid: self.fileid(index),
            modified: stat.modified,
            stat,
        })
    }

    /// The file id of a node: its place in the table of every export's
    /// nodes, from one, as `NFSv2` file ids are 32-bit and a file id of 0
    /// confuses some clients.
    fn fileid(&self, index: usize) -> u32 {
        u32::try_from(index + 1).ok().and_then(|i| i.checked_add(self.id_base)).unwrap_or(u32::MAX)
    }

    /// Builds the handle for a node.
    pub fn handle(&self, index: usize) -> Option<Handle> {
        let node = self.nodes.get(index)?;
        let mut out = [0_u8; HANDLE_LEN];
        out.get_mut(0..4)?.copy_from_slice(&self.fileid(index).to_be_bytes());
        out.get_mut(4..8)?.copy_from_slice(&self.fileid(node.parent).to_be_bytes());
        out.get_mut(8..12)?.copy_from_slice(&self.fileid(self.root()).to_be_bytes());
        Some(Handle(out))
    }

    /// Resolves a handle back to a node, rejecting anything we did not issue.
    pub fn node_of(&self, handle: &Handle) -> Option<usize> {
        let bytes = handle.as_bytes();
        let word = |at: usize| Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
        let (own, parent, root) = (word(0)?, word(4)?, word(8)?);
        let index = usize::try_from(own.checked_sub(self.id_base)?.checked_sub(1)?).ok()?;
        let node = self.nodes.get(index)?;
        if parent != self.fileid(node.parent) || root != self.fileid(self.root()) {
            return None;
        }
        // The trailing bytes must be the zeroes we issued: a handle that has
        // been tampered with anywhere is not one of ours.
        if bytes.get(12..).is_some_and(|rest| rest.iter().any(|b| *b != 0)) {
            return None;
        }
        Some(index)
    }
}

/// The whole set of exports a player can mount.
#[derive(Debug, Default, Clone)]
pub struct Exports {
    by_name: HashMap<String, Vfs>,
}

impl Exports {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an export, numbering its nodes after every export already in.
    pub fn insert(&mut self, mut vfs: Vfs) {
        let taken: usize = self.by_name.values().map(Vfs::len).sum();
        vfs.id_base = u32::try_from(taken).unwrap_or(u32::MAX);
        self.by_name.insert(vfs.export_name().to_owned(), vfs);
    }

    pub fn get(&self, name: &str) -> Option<&Vfs> {
        self.by_name.get(name)
    }

    /// Export names, sorted so a listing is stable between calls.
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.by_name.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}
