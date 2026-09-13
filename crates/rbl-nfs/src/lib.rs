//! A read-only NFS server, in the shape rekordbox presents one.
//!
//! Three RPC programs share one server: portmap, mount, and NFS version 2.
//! Everything here is a pure function from a request datagram to a reply
//! datagram, so the whole protocol is testable without a socket; binding the
//! sockets is the caller's job.
//!
//! # What differs from a standard NFS server
//!
//! - **Ports.** rekordbox answers portmap on **50111**, not 111, and NFS on
//!   2049 with no privileged port anywhere. Both are how the real software
//!   behaves, and a player finds the rest by asking portmap.
//! - **Names are UTF-16LE**, in both mount paths and file names.
//! - **Nothing is writable.** Every mutating procedure is refused with
//!   `NFSERR_ROFS` rather than left unimplemented, so a client that tries gets
//!   an answer it understands.

pub mod net;
pub mod rpc;
pub mod vfs;
pub mod xdr;

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub use vfs::{Attributes, Exports, Handle, NodeKind, Vfs, HANDLE_LEN, MAX_READ};
use xdr::{Reader, Writer};

/// The port rekordbox answers portmap on. A CDJ uses the standard 111; asking
/// rekordbox on 111 gets nothing, which is why this is not a fallback.
pub const REKORDBOX_PORTMAP_PORT: u16 = 50_111;
/// The standard portmap port, which hardware players use.
pub const STANDARD_PORTMAP_PORT: u16 = 111;
/// Where the NFS program itself listens.
pub const NFS_PORT: u16 = 2049;

pub const PROGRAM_PORTMAP: u32 = 100_000;
pub const PROGRAM_MOUNT: u32 = 100_005;
pub const PROGRAM_NFS: u32 = 100_003;

pub const VERSION_PORTMAP: u32 = 2;
pub const VERSION_MOUNT: u32 = 1;
pub const VERSION_NFS: u32 = 2;

pub const IPPROTO_TCP: u32 = 6;
pub const IPPROTO_UDP: u32 = 17;

/// Portmap procedures.
pub mod portmap_proc {
    pub const NULL: u32 = 0;
    pub const GETPORT: u32 = 3;
}

/// Mount procedures.
pub mod mount_proc {
    pub const NULL: u32 = 0;
    pub const MNT: u32 = 1;
    pub const DUMP: u32 = 2;
    pub const UMNT: u32 = 3;
    pub const UMNTALL: u32 = 4;
    pub const EXPORT: u32 = 5;
}

/// NFS version 2 procedures.
pub mod nfs_proc {
    pub const NULL: u32 = 0;
    pub const GETATTR: u32 = 1;
    pub const SETATTR: u32 = 2;
    pub const LOOKUP: u32 = 4;
    pub const READLINK: u32 = 5;
    pub const READ: u32 = 6;
    pub const WRITE: u32 = 8;
    pub const CREATE: u32 = 9;
    pub const REMOVE: u32 = 10;
    pub const RENAME: u32 = 11;
    pub const LINK: u32 = 12;
    pub const SYMLINK: u32 = 13;
    pub const MKDIR: u32 = 14;
    pub const RMDIR: u32 = 15;
    pub const READDIR: u32 = 16;
    pub const STATFS: u32 = 17;
}

/// NFS version 2 status codes.
pub mod nfs_status {
    pub const OK: u32 = 0;
    pub const PERM: u32 = 1;
    pub const NOENT: u32 = 2;
    pub const IO: u32 = 5;
    pub const ACCES: u32 = 13;
    pub const NOTDIR: u32 = 20;
    pub const ISDIR: u32 = 21;
    pub const ROFS: u32 = 30;
    pub const NAMETOOLONG: u32 = 63;
    pub const STALE: u32 = 70;
}

/// `NFSv2` file types.
mod file_type {
    pub const REGULAR: u32 = 1;
    pub const DIRECTORY: u32 = 2;
}

/// The longest single component a player may look up. `NFSv2`'s own limit.
pub const MAX_NAME: usize = 255;

/// A read-only NFS server over a set of exports.
#[derive(Debug)]
pub struct Server {
    exports: Exports,
    /// The port the NFS program is bound to, reported by portmap.
    nfs_port: u16,
    /// The port the mount program is bound to, reported by portmap.
    mount_port: u16,
    /// The files being read, kept open: a player reads a track in 32 KB
    /// pieces, and opening the file for each piece is a syscall and a
    /// directory walk per piece. Most recently used last.
    open: Mutex<Vec<OpenFile>>,
}

#[derive(Debug)]
struct OpenFile {
    path: PathBuf,
    file: File,
    used: std::time::Instant,
}

/// How many files stay open between reads: a player keeps two mounts and
/// reads one track through both, and a few players may load at once.
const OPEN_FILES: usize = 8;
/// A handle unused for this long is closed, so a file replaced on disk is
/// read afresh rather than from the old inode for as long as it is cached.
const OPEN_FOR: std::time::Duration = std::time::Duration::from_secs(30);

impl Server {
    pub fn new(exports: Exports, nfs_port: u16, mount_port: u16) -> Self {
        Self { exports, nfs_port, mount_port, open: Mutex::new(Vec::with_capacity(OPEN_FILES)) }
    }

    /// Reads `len` bytes at `offset` of the file at `path`, through the
    /// open-file cache.
    fn read_at(&self, path: &Path, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
        let mut open = self.open.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = std::time::Instant::now();
        open.retain(|entry| now.duration_since(entry.used) < OPEN_FOR);
        let at = if let Some(at) = open.iter().position(|entry| entry.path == path) {
            at
        } else {
            if open.len() >= OPEN_FILES {
                open.remove(0);
            }
            open.push(OpenFile { path: path.to_path_buf(), file: File::open(path)?, used: now });
            open.len() - 1
        };
        let mut entry = open.remove(at);
        let outcome = read_at(&entry.file, offset, len);
        // A read that failed drops the handle: the file may have been
        // replaced, and the next read opens whatever is there now.
        if outcome.is_ok() {
            entry.used = now;
            open.push(entry);
        }
        outcome
    }

    pub fn exports(&self) -> &Exports {
        &self.exports
    }

    /// Answers one request datagram.
    ///
    /// `None` means "say nothing": the datagram was not an RPC call we can
    /// even address a reply to. A malformed call we *can* identify still gets
    /// a reply, because silence is what a client times out on.
    pub fn handle(&self, datagram: &[u8]) -> Option<Vec<u8>> {
        let call = match rpc::Call::decode(datagram) {
            Ok(call) => call,
            Err(rpc::RpcError::Version(_)) => {
                // We can still read the xid: tell it which version we speak.
                let xid = u32::from_be_bytes([
                    *datagram.first()?,
                    *datagram.get(1)?,
                    *datagram.get(2)?,
                    *datagram.get(3)?,
                ]);
                return Some(rpc::rpc_mismatch(xid, rpc::RPC_VERSION, rpc::RPC_VERSION));
            }
            Err(_) => return None,
        };

        Some(match call.program {
            PROGRAM_PORTMAP => self.portmap(&call),
            PROGRAM_MOUNT => self.mount(&call),
            PROGRAM_NFS => self.nfs(&call),
            _ => rpc::accepted_empty(call.xid, rpc::accept::PROG_UNAVAIL),
        })
    }

    fn portmap(&self, call: &rpc::Call<'_>) -> Vec<u8> {
        if call.version != VERSION_PORTMAP {
            return rpc::program_mismatch(call.xid, VERSION_PORTMAP, VERSION_PORTMAP);
        }
        match call.procedure {
            portmap_proc::NULL => rpc::accepted_empty(call.xid, rpc::accept::SUCCESS),
            portmap_proc::GETPORT => {
                let mut reader = call.reader();
                let (Ok(program), Ok(_version), Ok(protocol)) =
                    (reader.u32(), reader.u32(), reader.u32())
                else {
                    return rpc::accepted_empty(call.xid, rpc::accept::GARBAGE_ARGS);
                };
                // Zero means "not registered", which is the correct answer for
                // a program we do not serve, and for TCP, which we do not bind.
                let port = if protocol == IPPROTO_UDP {
                    match program {
                        PROGRAM_NFS => self.nfs_port,
                        PROGRAM_MOUNT => self.mount_port,
                        _ => 0,
                    }
                } else {
                    0
                };
                let mut writer = rpc::accepted(call.xid, rpc::accept::SUCCESS);
                writer.u32(u32::from(port));
                writer.into_bytes()
            }
            _ => rpc::accepted_empty(call.xid, rpc::accept::PROC_UNAVAIL),
        }
    }

    fn mount(&self, call: &rpc::Call<'_>) -> Vec<u8> {
        if call.version != VERSION_MOUNT {
            return rpc::program_mismatch(call.xid, VERSION_MOUNT, VERSION_MOUNT);
        }
        match call.procedure {
            // Unmounting has nothing to undo: we hold no per-client state, so
            // a client that never unmounts costs us nothing.
            mount_proc::NULL | mount_proc::UMNT | mount_proc::UMNTALL => {
                rpc::accepted_empty(call.xid, rpc::accept::SUCCESS)
            }
            mount_proc::MNT => {
                let mut reader = call.reader();
                let Ok(path) = reader.utf16() else {
                    return rpc::accepted_empty(call.xid, rpc::accept::GARBAGE_ARGS);
                };
                let mut writer = rpc::accepted(call.xid, rpc::accept::SUCCESS);
                match self.exports.get(&path).and_then(|vfs| vfs.handle(vfs.root())) {
                    Some(handle) => {
                        writer.u32(nfs_status::OK).opaque_fixed(handle.as_bytes());
                    }
                    None => {
                        writer.u32(nfs_status::NOENT);
                    }
                }
                writer.into_bytes()
            }
            mount_proc::DUMP | mount_proc::EXPORT => {
                let mut writer = rpc::accepted(call.xid, rpc::accept::SUCCESS);
                for name in self.exports.names() {
                    // Each entry is an optional-list link: present, name, then
                    // an empty group list.
                    writer.some().utf16(name).none();
                }
                writer.none();
                writer.into_bytes()
            }
            _ => rpc::accepted_empty(call.xid, rpc::accept::PROC_UNAVAIL),
        }
    }

    fn nfs(&self, call: &rpc::Call<'_>) -> Vec<u8> {
        if call.version != VERSION_NFS {
            return rpc::program_mismatch(call.xid, VERSION_NFS, VERSION_NFS);
        }
        match call.procedure {
            nfs_proc::NULL => rpc::accepted_empty(call.xid, rpc::accept::SUCCESS),
            nfs_proc::GETATTR => self.getattr(call),
            nfs_proc::LOOKUP => self.lookup(call),
            nfs_proc::READ => self.read(call),
            nfs_proc::READDIR => self.readdir(call),
            nfs_proc::STATFS => self.statfs(call),
            // Every mutating procedure, answered rather than ignored.
            nfs_proc::SETATTR
            | nfs_proc::WRITE
            | nfs_proc::CREATE
            | nfs_proc::REMOVE
            | nfs_proc::RENAME
            | nfs_proc::LINK
            | nfs_proc::SYMLINK
            | nfs_proc::MKDIR
            | nfs_proc::RMDIR => Self::status_only(call.xid, nfs_status::ROFS),
            // Nothing in an export is a symlink, so a readlink is always wrong.
            nfs_proc::READLINK => Self::status_only(call.xid, nfs_status::PERM),
            _ => rpc::accepted_empty(call.xid, rpc::accept::PROC_UNAVAIL),
        }
    }

    fn status_only(xid: u32, status: u32) -> Vec<u8> {
        let mut writer = rpc::accepted(xid, rpc::accept::SUCCESS);
        writer.u32(status);
        writer.into_bytes()
    }

    /// Finds the export a handle belongs to. A handle carries no export id, so
    /// this asks each in turn — the tag check makes a wrong export reject it.
    fn locate(&self, handle: &Handle) -> Option<(&Vfs, usize)> {
        self.exports
            .names()
            .into_iter()
            .filter_map(|name| self.exports.get(name))
            .find_map(|vfs| vfs.node_of(handle).map(|index| (vfs, index)))
    }

    fn read_handle(reader: &mut Reader<'_>) -> Option<Handle> {
        Handle::from_slice(reader.opaque_fixed(HANDLE_LEN).ok()?)
    }

    fn getattr(&self, call: &rpc::Call<'_>) -> Vec<u8> {
        let mut reader = call.reader();
        let Some(handle) = Self::read_handle(&mut reader) else {
            return rpc::accepted_empty(call.xid, rpc::accept::GARBAGE_ARGS);
        };
        let Some((vfs, index)) = self.locate(&handle) else {
            return Self::status_only(call.xid, nfs_status::STALE);
        };
        let Some(attributes) = vfs.attributes(index) else {
            return Self::status_only(call.xid, nfs_status::STALE);
        };
        let mut writer = rpc::accepted(call.xid, rpc::accept::SUCCESS);
        writer.u32(nfs_status::OK);
        write_attributes(&mut writer, &attributes);
        writer.into_bytes()
    }

    fn lookup(&self, call: &rpc::Call<'_>) -> Vec<u8> {
        let mut reader = call.reader();
        let Some(handle) = Self::read_handle(&mut reader) else {
            return rpc::accepted_empty(call.xid, rpc::accept::GARBAGE_ARGS);
        };
        let Ok(name) = reader.utf16() else {
            return rpc::accepted_empty(call.xid, rpc::accept::GARBAGE_ARGS);
        };
        if name.len() > MAX_NAME {
            return Self::status_only(call.xid, nfs_status::NAMETOOLONG);
        }
        let Some((vfs, parent)) = self.locate(&handle) else {
            return Self::status_only(call.xid, nfs_status::STALE);
        };
        if vfs.kind(parent) != Some(NodeKind::Directory) {
            return Self::status_only(call.xid, nfs_status::NOTDIR);
        }
        let found = vfs
            .child(parent, &name)
            .and_then(|index| Some((vfs.handle(index)?, vfs.attributes(index)?)));
        let Some((child_handle, attributes)) = found else {
            return Self::status_only(call.xid, nfs_status::NOENT);
        };
        let mut writer = rpc::accepted(call.xid, rpc::accept::SUCCESS);
        writer.u32(nfs_status::OK).opaque_fixed(child_handle.as_bytes());
        write_attributes(&mut writer, &attributes);
        writer.into_bytes()
    }

    fn read(&self, call: &rpc::Call<'_>) -> Vec<u8> {
        let mut reader = call.reader();
        let Some(handle) = Self::read_handle(&mut reader) else {
            return rpc::accepted_empty(call.xid, rpc::accept::GARBAGE_ARGS);
        };
        let (Ok(offset), Ok(count)) = (reader.u32(), reader.u32()) else {
            return rpc::accepted_empty(call.xid, rpc::accept::GARBAGE_ARGS);
        };
        let Some((vfs, index)) = self.locate(&handle) else {
            return Self::status_only(call.xid, nfs_status::STALE);
        };
        if vfs.kind(index) == Some(NodeKind::Directory) {
            return Self::status_only(call.xid, nfs_status::ISDIR);
        }
        let (Some(source), Some(attributes)) = (vfs.source(index), vfs.attributes(index)) else {
            return Self::status_only(call.xid, nfs_status::STALE);
        };

        let wanted = (count as usize).min(MAX_READ);
        // A file that vanished between the export and the read is the normal
        // case here, not an I/O fault worth distinguishing.
        let Ok(data) = self.read_at(source, u64::from(offset), wanted) else {
            return Self::status_only(call.xid, nfs_status::IO);
        };

        let mut writer = rpc::accepted(call.xid, rpc::accept::SUCCESS);
        writer.u32(nfs_status::OK);
        write_attributes(&mut writer, &attributes);
        writer.opaque(&data);
        writer.into_bytes()
    }

    fn readdir(&self, call: &rpc::Call<'_>) -> Vec<u8> {
        let mut reader = call.reader();
        let Some(handle) = Self::read_handle(&mut reader) else {
            return rpc::accepted_empty(call.xid, rpc::accept::GARBAGE_ARGS);
        };
        let (Ok(cookie), Ok(count)) = (reader.u32(), reader.u32()) else {
            return rpc::accepted_empty(call.xid, rpc::accept::GARBAGE_ARGS);
        };
        let Some((vfs, index)) = self.locate(&handle) else {
            return Self::status_only(call.xid, nfs_status::STALE);
        };
        if vfs.kind(index) != Some(NodeKind::Directory) {
            return Self::status_only(call.xid, nfs_status::NOTDIR);
        }

        let children = vfs.children(index);
        let mut writer = rpc::accepted(call.xid, rpc::accept::SUCCESS);
        writer.u32(nfs_status::OK);

        // The cookie is the index of the next child to send, so a resumed
        // listing continues exactly where the previous reply stopped.
        let budget = (count as usize).clamp(512, 8192);
        let mut at = cookie as usize;
        while let Some(child) = children.get(at) {
            let (Some(name), Some(attributes)) = (vfs.name(*child), vfs.attributes(*child)) else {
                break;
            };
            // Entry size: present flag, fileid, name (length + padded UTF-16LE),
            // cookie. Stop before overrunning what the client asked for.
            let entry_len = 4 + 4 + 4 + xdr::padded(name.len() * 2) + 4;
            if writer.len() + entry_len + 8 > budget {
                break;
            }
            at += 1;
            writer.some().u32(attributes.fileid).utf16(name).u32(
                u32::try_from(at).unwrap_or(u32::MAX),
            );
        }
        let eof = at >= children.len();
        writer.none().u32(u32::from(eof));
        writer.into_bytes()
    }

    fn statfs(&self, call: &rpc::Call<'_>) -> Vec<u8> {
        let mut reader = call.reader();
        let Some(handle) = Self::read_handle(&mut reader) else {
            return rpc::accepted_empty(call.xid, rpc::accept::GARBAGE_ARGS);
        };
        if self.locate(&handle).is_none() {
            return Self::status_only(call.xid, nfs_status::STALE);
        }
        let mut writer = rpc::accepted(call.xid, rpc::accept::SUCCESS);
        // Sizes are advisory. Free space is reported as zero, which is honest:
        // the export is read-only, so nothing can be written into it.
        writer
            .u32(nfs_status::OK)
            .u32(u32::try_from(MAX_READ).unwrap_or(8192)) // tsize
            .u32(4096) // bsize
            .u32(u32::MAX) // blocks
            .u32(0) // bfree
            .u32(0); // bavail
        writer.into_bytes()
    }
}

/// Writes an `NFSv2` `fattr`: seventeen 32-bit fields, no padding.
fn write_attributes(writer: &mut Writer, attributes: &Attributes) {
    let (kind, mode, nlink) = match attributes.kind {
        // 0o40555 and 0o100444: readable and traversable, never writable.
        NodeKind::Directory => (file_type::DIRECTORY, 0o040_555, 2),
        NodeKind::File => (file_type::REGULAR, 0o100_444, 1),
    };
    let size = u32::try_from(attributes.size).unwrap_or(u32::MAX);
    writer
        .u32(kind)
        .u32(mode)
        .u32(nlink)
        .u32(0) // uid
        .u32(0) // gid
        .u32(size)
        .u32(4096) // blocksize
        .u32(0) // rdev
        .u32(size.div_ceil(512)) // blocks
        .u32(1) // fsid
        .u32(attributes.fileid);
    for _ in 0..3 {
        writer.u32(attributes.modified).u32(0);
    }
}

/// Reads at most `len` bytes from `offset`. A short read at the end of the
/// file is not an error; NFS signals the end by returning fewer bytes.
fn read_at(mut file: &File, offset: u64, len: usize) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut out = vec![0_u8; len];
    let mut filled = 0;
    while filled < len {
        match file.read(out.get_mut(filled..).unwrap_or(&mut []))? {
            0 => break,
            n => filled += n,
        }
    }
    out.truncate(filled);
    Ok(out)
}

/// Splits an absolute path into the export a player mounts and the path within
/// it, following rekordbox's own convention.
///
/// macOS exports `/`, so `/Users/x/a.mp3` mounts `/` and reads `Users/x/a.mp3`.
/// Windows exports the drive, so `C:\Users\x\a.mp3` mounts `/C/` and reads
/// `Users/x/a.mp3`.
pub fn split_export(path: &str) -> Option<(String, String)> {
    let mut chars = path.chars();
    let (Some(drive), Some(':'), Some(separator)) = (chars.next(), chars.next(), chars.next())
    else {
        return path
            .strip_prefix('/')
            .map(|rest| ("/".to_owned(), rest.replace('\\', "/")));
    };
    if separator != '/' && separator != '\\' {
        return None;
    }
    let rest: String = chars.collect();
    Some((
        format!("/{}/", drive.to_ascii_uppercase()),
        rest.replace('\\', "/"),
    ))
}
