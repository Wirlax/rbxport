//! The read-only tree a player sees, and the file handles that address it.
//!
//! The tree is built once from the export plan; a player can only reach a node
//! by walking from the mount point, one name at a time. Names are never joined
//! onto a path and re-opened, so `..`, an absolute name, or a symlink in a name
//! cannot escape the export — there is nothing to escape *to*, because the only
//! real paths in existence are the ones we registered.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A file handle is a fixed 32 opaque bytes in `NFSv2`.
pub const HANDLE_LEN: usize = 32;

/// The most a single `READ` may return. `NFSv2`'s own limit, and what a CDJ asks for.
pub const MAX_READ: usize = 8192;

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
    size: u64,
    /// Seconds since the epoch, for all three timestamps.
    modified: u32,
}

/// An exported filesystem and everything reachable inside it.
#[derive(Debug, Clone)]
pub struct Vfs {
    /// The export name a player mounts, e.g. `/` on macOS or `/C/` on Windows.
    export: String,
    nodes: Vec<Node>,
    /// Salt mixed into every handle so an index alone is not a valid handle.
    salt: u64,
}

/// Addresses one node. Opaque to the player; checked on the way back in.
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

/// FNV-1a. Small, dependency-free, and adequate: the tag exists so a handle
/// cannot be forged by guessing an index, not to resist a cryptographic attack.
fn fnv1a(seed: u64, bytes: &[u8]) -> u64 {
    let mut hash = seed ^ 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

/// Node attributes, as `NFSv2` reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attributes {
    pub kind: NodeKind,
    pub size: u64,
    pub fileid: u32,
    pub modified: u32,
}

impl Vfs {
    /// Creates an empty export. `export` is the name a player mounts.
    pub fn new(export: impl Into<String>) -> Self {
        let export = export.into();
        let salt = fnv1a(0, export.as_bytes());
        Self {
            export,
            nodes: vec![Node {
                name: String::new(),
                kind: NodeKind::Directory,
                parent: 0,
                children: Vec::new(),
                source: None,
                size: 0,
                modified: 0,
            }],
            salt,
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
            at = self.child_or_insert(at, part, NodeKind::Directory, None, 0, modified);
        }
        self.child_or_insert(at, last, NodeKind::File, Some(source.into()), size, modified)
    }

    /// Adds an empty directory, for a tree that must show a folder with no files.
    pub fn add_dir(&mut self, path: &str) -> usize {
        let mut at = self.root();
        for part in path.split('/').filter(|p| !p.is_empty() && *p != "." && *p != "..") {
            at = self.child_or_insert(at, part, NodeKind::Directory, None, 0, 0);
        }
        at
    }

    fn child_or_insert(
        &mut self,
        parent: usize,
        name: &str,
        kind: NodeKind,
        source: Option<PathBuf>,
        size: u64,
        modified: u32,
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
            size,
            modified,
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
        node.children
            .iter()
            .copied()
            .find(|index| self.nodes.get(*index).is_some_and(|child| child.name == name))
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
        Some(Attributes {
            kind: node.kind,
            size: node.size,
            // NFSv2 file ids are 32-bit; the index is dense and starts at zero,
            // and a fileid of 0 confuses some clients, so it starts at one.
            fileid: u32::try_from(index + 1).unwrap_or(u32::MAX),
            modified: node.modified,
        })
    }

    /// Builds the handle for a node.
    pub fn handle(&self, index: usize) -> Option<Handle> {
        if index >= self.nodes.len() {
            return None;
        }
        let index32 = u32::try_from(index).ok()?;
        let mut out = [0_u8; HANDLE_LEN];
        out.get_mut(0..4)?.copy_from_slice(&index32.to_be_bytes());
        let tag = fnv1a(self.salt, &index32.to_be_bytes());
        out.get_mut(4..12)?.copy_from_slice(&tag.to_be_bytes());
        Some(Handle(out))
    }

    /// Resolves a handle back to a node, rejecting anything we did not issue.
    pub fn node_of(&self, handle: &Handle) -> Option<usize> {
        let bytes = handle.as_bytes();
        let index32 = u32::from_be_bytes([
            *bytes.first()?,
            *bytes.get(1)?,
            *bytes.get(2)?,
            *bytes.get(3)?,
        ]);
        let tag = u64::from_be_bytes(bytes.get(4..12)?.try_into().ok()?);
        if tag != fnv1a(self.salt, &index32.to_be_bytes()) {
            return None;
        }
        let index = index32 as usize;
        if index >= self.nodes.len() {
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

    pub fn insert(&mut self, vfs: Vfs) {
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
