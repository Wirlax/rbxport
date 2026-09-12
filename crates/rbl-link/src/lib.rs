//! Link export: the library served to players over Pro DJ Link, the way
//! rekordbox serves it.
//!
//! Three servers and a beacon, all measured against rekordbox 7.2.11 and a
//! CDJ-3000 (`docs/pre-release/design-notes/link-export-capture.md`): the
//! keep-alive and status packets that put us on the network as `rekordbox`,
//! the database server a player browses, and the NFS server it reads the
//! audio file from. `catalog` answers the database server's questions out of
//! the index; `blobs` builds the analysis replies out of the ANLZ files.

pub mod blobs;
pub mod catalog;
