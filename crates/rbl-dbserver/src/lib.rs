//! The remote-database protocol a CDJ uses to browse another device's library.
//!
//! A client opens a TCP connection, asks port 12523 which port the database
//! server is on, then exchanges framed messages: a magic, a transaction id, a
//! type, and up to twelve typed arguments.
//!
//! # Verified and unverified
//!
//! The framing and field encodings here follow the community protocol analysis
//! and are tested by round-tripping. What is **not** yet verified is the part
//! that only a capture can settle: which menus rekordbox actually exposes as a
//! source, and the exact item-type codes a player accepts. Those are marked
//! [`Unverified`] and must be confirmed before a player is expected to browse us.
//!
//! Nothing here opens a socket; it is a codec, so it is testable exhaustively.

pub mod net;

/// Every message starts with this.
pub const MAGIC: u32 = 0x8723_49ae;

/// The port a client asks for the database server's real port.
pub const PORT_QUERY: u16 = 12_523;

/// What a player sends to the port-query service: a four-byte big-endian
/// length, then the name with its NUL (measured: `00 00 00 0f RemoteDBServer 00`).
pub const PORT_QUERY_REQUEST: &[u8] = b"\x00\x00\x00\x0fRemoteDBServer\0";

/// The five bytes each side sends first on the database connection
/// (measured; a number field holding 1).
pub const GREETING: &[u8] = &[0x11, 0x00, 0x00, 0x00, 0x01];

/// Transaction id used for setup and teardown.
pub const SETUP_TXID: u32 = 0xffff_fffe;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DbError {
    #[error("message is truncated: wanted {wanted} bytes, had {had}")]
    Truncated { wanted: usize, had: usize },
    #[error("not a database message: magic was {0:#010x}")]
    BadMagic(u32),
    #[error("unknown argument tag {0:#04x}")]
    UnknownTag(u8),
    #[error("message declares {0} arguments, which is more than twelve")]
    TooManyArguments(u8),
}

pub type Result<T> = std::result::Result<T, DbError>;

/// A message argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Argument {
    /// A four-byte big-endian integer.
    Number(u32),
    /// UTF-16BE text.
    String(String),
    Blob(Vec<u8>),
}

impl Argument {
    /// The tag that appears in the header's type list.
    fn arg_tag(&self) -> u8 {
        match self {
            Self::String(_) => 0x02,
            Self::Blob(_) => 0x03,
            Self::Number(_) => 0x06,
        }
    }

    /// The tag that prefixes the value itself. Note it differs from the
    /// header tag — the protocol carries the type twice, in two encodings.
    fn field_tag(&self) -> u8 {
        match self {
            Self::Number(_) => 0x11,
            Self::Blob(_) => 0x14,
            Self::String(_) => 0x26,
        }
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.field_tag());
        match self {
            Self::Number(v) => out.extend_from_slice(&v.to_be_bytes()),
            Self::Blob(bytes) => {
                out.extend_from_slice(&u32::try_from(bytes.len()).unwrap_or(0).to_be_bytes());
                out.extend_from_slice(bytes);
            }
            Self::String(text) => {
                // The length counts UTF-16 code units including the trailing NUL.
                let units: Vec<u16> = text.encode_utf16().collect();
                let count = u32::try_from(units.len() + 1).unwrap_or(0);
                out.extend_from_slice(&count.to_be_bytes());
                for unit in units {
                    out.extend_from_slice(&unit.to_be_bytes());
                }
                out.extend_from_slice(&[0, 0]);
            }
        }
    }
}

/// One framed message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub transaction: u32,
    pub kind: u16,
    pub arguments: Vec<Argument>,
}

/// Bytes of header before the tag list: each field carries its own type tag
/// — `11` magic, `11` transaction, `10` kind, `0f` argument count — and then
/// the tag list opens as a blob (`14`, four-byte length). Measured from
/// rekordbox 7.2.11 serving a CDJ-3000 (`verification/link`, 2026-09-12): the
/// earlier layout here had no field tags and a fixed twelve-byte tag list,
/// and matched nothing on the wire.
const HEADER_LEN: usize = 1 + 4 + 1 + 4 + 1 + 2 + 1 + 1 + 1 + 4;
/// The most arguments a message may carry; the tag list holds one byte each.
/// A menu item carries sixteen.
const TAG_SLOTS: usize = 32;

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([
        b.get(at).copied().unwrap_or(0),
        b.get(at + 1).copied().unwrap_or(0),
        b.get(at + 2).copied().unwrap_or(0),
        b.get(at + 3).copied().unwrap_or(0),
    ])
}

impl Message {
    pub fn new(transaction: u32, kind: u16, arguments: Vec<Argument>) -> Self {
        Self { transaction, kind, arguments }
    }

    pub fn encode(&self) -> Vec<u8> {
        let count = self.arguments.len().min(TAG_SLOTS);
        let mut out = Vec::with_capacity(HEADER_LEN + count * 8);
        out.push(0x11);
        out.extend_from_slice(&MAGIC.to_be_bytes());
        out.push(0x11);
        out.extend_from_slice(&self.transaction.to_be_bytes());
        out.push(0x10);
        out.extend_from_slice(&self.kind.to_be_bytes());
        out.push(0x0f);
        out.push(u8::try_from(count).unwrap_or(0));
        // The tag list is itself a blob field, one byte an argument.
        out.push(0x14);
        out.extend_from_slice(&u32::try_from(count).unwrap_or(0).to_be_bytes());
        for argument in self.arguments.iter().take(count) {
            out.push(argument.arg_tag());
        }
        for argument in self.arguments.iter().take(count) {
            // An empty blob is declared in the tag list and not written; the
            // number before it, its length, says it is absent.
            if matches!(argument, Argument::Blob(b) if b.is_empty()) {
                continue;
            }
            argument.encode(&mut out);
        }
        out
    }

    /// Decodes one message, returning it and how many bytes it consumed.
    ///
    /// Returning the length is what lets a reader handle several messages
    /// arriving in one TCP segment, and detect one split across segments.
    pub fn decode(bytes: &[u8]) -> Result<(Self, usize)> {
        if bytes.len() < HEADER_LEN {
            return Err(DbError::Truncated { wanted: HEADER_LEN, had: bytes.len() });
        }
        // The field tags are checked as part of the magic: a stream that is
        // not at a message boundary fails here rather than misreading a body.
        let magic = be32(bytes, 1);
        if bytes[0] != 0x11 || magic != MAGIC || bytes[5] != 0x11 || bytes[10] != 0x10
            || bytes[13] != 0x0f || bytes[15] != 0x14
        {
            return Err(DbError::BadMagic(magic));
        }
        let transaction = be32(bytes, 6);
        let kind = u16::from_be_bytes([
            bytes.get(11).copied().unwrap_or(0),
            bytes.get(12).copied().unwrap_or(0),
        ]);
        let count = bytes.get(14).copied().unwrap_or(0);
        if count as usize > TAG_SLOTS {
            return Err(DbError::TooManyArguments(count));
        }
        let tags = be32(bytes, 16) as usize;
        if tags != count as usize {
            return Err(DbError::BadMagic(magic));
        }

        let mut at = HEADER_LEN + tags;
        if bytes.len() < at {
            return Err(DbError::Truncated { wanted: at, had: bytes.len() });
        }
        let declared: Vec<u8> = bytes.get(HEADER_LEN..at).unwrap_or(&[]).to_vec();
        let mut arguments = Vec::with_capacity(count as usize);
        for declared_tag in declared {
            // An empty blob is not sent at all. The number before a blob is
            // always its length, and when that number is 0 the blob field is
            // absent — the "no artwork" reply declares four arguments and
            // carries three, and a player's metadata request declares five
            // and carries four. [DOC] Deep Symmetry, track_metadata; measured
            // both ways in the capture.
            if declared_tag == 0x03 && matches!(arguments.last(), Some(Argument::Number(0)) | None) {
                arguments.push(Argument::Blob(Vec::new()));
                continue;
            }
            let tag = bytes.get(at).copied().ok_or(DbError::Truncated { wanted: at + 1, had: bytes.len() })?;
            at += 1;
            match tag {
                0x0f..=0x11 => {
                    // Numbers are one, two or four bytes depending on the tag.
                    let width = match tag {
                        0x0f => 1,
                        0x10 => 2,
                        _ => 4,
                    };
                    if at + width > bytes.len() {
                        return Err(DbError::Truncated { wanted: at + width, had: bytes.len() });
                    }
                    let value = match width {
                        1 => u32::from(bytes.get(at).copied().unwrap_or(0)),
                        2 => u32::from(u16::from_be_bytes([
                            bytes.get(at).copied().unwrap_or(0),
                            bytes.get(at + 1).copied().unwrap_or(0),
                        ])),
                        _ => be32(bytes, at),
                    };
                    arguments.push(Argument::Number(value));
                    at += width;
                }
                0x14 => {
                    let len = be32(bytes, at) as usize;
                    at += 4;
                    if at + len > bytes.len() {
                        return Err(DbError::Truncated { wanted: at + len, had: bytes.len() });
                    }
                    arguments.push(Argument::Blob(
                        bytes.get(at..at + len).unwrap_or(&[]).to_vec(),
                    ));
                    at += len;
                }
                0x26 => {
                    // The count is UTF-16 code units including the trailing NUL.
                    let units = be32(bytes, at) as usize;
                    at += 4;
                    let byte_len = units.saturating_mul(2);
                    if at + byte_len > bytes.len() {
                        return Err(DbError::Truncated {
                            wanted: at + byte_len,
                            had: bytes.len(),
                        });
                    }
                    let raw = bytes.get(at..at + byte_len).unwrap_or(&[]);
                    let text: Vec<u16> = raw
                        .chunks_exact(2)
                        .map(|c| u16::from_be_bytes([c[0], c[1]]))
                        .take_while(|&u| u != 0)
                        .collect();
                    arguments.push(Argument::String(String::from_utf16_lossy(&text)));
                    at += byte_len;
                }
                other => return Err(DbError::UnknownTag(other)),
            }
        }

        Ok((Self { transaction, kind, arguments }, at))
    }

    /// Decodes as many whole messages as the buffer holds, returning them and
    /// how many bytes were consumed. A partial trailing message is left behind.
    pub fn decode_all(bytes: &[u8]) -> (Vec<Self>, usize) {
        let mut out = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            match Self::decode(bytes.get(at..).unwrap_or(&[])) {
                Ok((message, used)) if used > 0 => {
                    out.push(message);
                    at += used;
                }
                // Truncated means the rest is a partial message: stop and keep it.
                _ => break,
            }
        }
        (out, at)
    }
}

/// Message types.
///
/// Request codes come from the protocol analysis. Which of these rekordbox
/// answers as a source, and with what item types, is [`Unverified`].
pub mod kind {
    /// Opens a session; the reply carries the server's own device number.
    pub const SETUP: u16 = 0x0000;
    /// Closes a session.
    pub const TEARDOWN: u16 = 0x0100;
    /// The top-level menu for a media slot.
    pub const ROOT_MENU: u16 = 0x1000;
    /// Playlists and folders.
    pub const PLAYLIST_MENU: u16 = 0x1105;
    /// Tracks by title.
    pub const TRACK_MENU: u16 = 0x1004;
    /// Search by text.
    pub const SEARCH: u16 = 0x1300;
    /// Metadata for one track.
    pub const METADATA: u16 = 0x2002;
    /// Album art.
    pub const ARTWORK: u16 = 0x2003;
    /// The beat grid.
    pub const BEAT_GRID: u16 = 0x2204;
    /// Cues and loops.
    pub const CUES: u16 = 0x2104;
    /// A whole analysis tag, e.g. a colour waveform.
    pub const ANLZ_TAG: u16 = 0x2c04;
    /// Track information: the path and the copyright text (7 rows).
    pub const TRACK_INFO: u16 = 0x2102;
    /// Asks for the rows of the menu just requested.
    pub const RENDER: u16 = 0x3000;
    /// The player tells us which of our tracks it has loaded.
    pub const LOADED: u16 = 0x3100;
    /// "Here is how many items your query matched."
    pub const MENU_HEADER: u16 = 0x4000;
    /// Opens a rendered menu: `[1, offset]`.
    pub const RENDER_HEADER: u16 = 0x4001;
    /// One row of a menu.
    pub const MENU_ITEM: u16 = 0x4101;
    /// End of a menu.
    pub const MENU_FOOTER: u16 = 0x4201;
    /// The query failed.
    pub const ERROR: u16 = 0x4003;
}

/// Things a capture must settle before a real player is expected to browse us.
///
/// Recorded as a list rather than as comments so it can be worked through.
pub const UNVERIFIED: &[&str] = &[
    "which root-menu item types rekordbox exposes as a source, versus a CDJ media slot",
    "how rekordbox advertises its database port back over 12523",
    "the exact MenuHeader/MenuItem/MenuFooter sequence a player accepts",
    "the item set and path format returned for a track-info request",
];

/// A requester device number a real database server will answer.
///
/// Servers answer 1 through 6 and silently ignore 7 and above — a client that
/// picks a high number sees the connection succeed and then nothing happen,
/// which is confusing enough to be worth rejecting explicitly.
pub fn is_answerable_device(device: u8) -> bool {
    (1..=6).contains(&device)
}

/// Builds the setup message a client sends first.
pub fn setup_request(device: u8) -> Message {
    Message::new(SETUP_TXID, kind::SETUP, vec![Argument::Number(u32::from(device)), Argument::Number(SETUP_MAGIC)])
}

/// The second setup argument, sent by both sides; meaning unknown, value
/// measured.
pub const SETUP_MAGIC: u32 = 0x14;

/// Builds the reply to a setup message, carrying our own device number.
///
/// The reply has the request's own kind, not a menu header (measured).
pub fn setup_reply(transaction: u32, our_device: u8) -> Message {
    Message::new(
        transaction,
        kind::SETUP,
        vec![Argument::Number(u32::from(our_device)), Argument::Number(SETUP_MAGIC)],
    )
}

/// Builds the header that tells a client how many rows its query matched.
pub fn menu_header(transaction: u32, item_count: u32) -> Message {
    Message::new(
        transaction,
        kind::MENU_HEADER,
        vec![Argument::Number(0), Argument::Number(item_count)],
    )
}

/// Builds the footer that ends a menu.
pub fn menu_footer(transaction: u32) -> Message {
    Message::new(transaction, kind::MENU_FOOTER, vec![Argument::Number(0), Argument::Number(0)])
}
