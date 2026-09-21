//! One player's conversation with the database server.
//!
//! A menu request is answered with a row count; the rows follow on a render
//! request that names an offset and a limit, so the session keeps the menu
//! it was last asked for. Every layout here is the one rekordbox 7.2.11
//! sent a CDJ-3000 (`docs/pre-release/design-notes/link-export-capture.md`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use crate::catalog::{Analysis, Catalog, Edit, Query, Row, Sort, TrackDetails, TrackScope};
use crate::item::{item_type, root_menu, sort_menu, track_flags, Item};
use crate::net::{Handler, Session};
use crate::{keys, kind, menu_footer, menu_header, setup_reply, Argument, Message};

/// Our device number on the link when nothing has settled one: rekordbox's
/// first choice, so a player treats us as it treats rekordbox.
pub const DEVICE: u8 = 0x11;

/// Serves a catalog to every player that connects.
pub struct CatalogHandler {
    catalog: Arc<dyn Catalog>,
    /// The device number the beacon's join settled on — 17, or 18 when
    /// another rekordbox holds 17 — and `0` while there is none, which is
    /// also what keeps the port query unanswered until the link is up.
    device: Arc<AtomicU8>,
}

impl CatalogHandler {
    pub fn new(catalog: Arc<dyn Catalog>) -> Self {
        Self { catalog, device: Arc::new(AtomicU8::new(DEVICE)) }
    }

    /// Answers with the number in `device` rather than the fixed 17.
    #[must_use]
    pub fn with_device(mut self, device: Arc<AtomicU8>) -> Self {
        self.device = device;
        self
    }
}

impl Handler for CatalogHandler {
    fn open(&self) -> Box<dyn Session> {
        let device = match self.device.load(Ordering::Relaxed) {
            0 => DEVICE,
            number => number,
        };
        Box::new(LinkSession::new(Arc::clone(&self.catalog)).as_device(device))
    }

    fn serving(&self) -> bool {
        self.device.load(Ordering::Relaxed) != 0
    }
}

/// The length of rekordbox's user-info blob.
const USER_INFO_LEN: usize = 160;

/// The rows of the menu a player last asked for, ready to render.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Menu {
    Root,
    SortOptions,
    Keys,
    RelatedKeys(u32),
    Library { query: Query, rows: Vec<Row> },
    Metadata(Box<TrackDetails>),
    TrackInfo(Box<TrackDetails>),
    /// What the player delivers to KUVO about a track it loaded.
    DeliveryInfo(Box<TrackDetails>),
    /// A request with no rows: `3007`, MATCHING, an unknown track.
    Empty,
}

impl Menu {
    fn len(&self) -> u32 {
        let n = match self {
            Self::Root => root_menu().len(),
            Self::SortOptions => sort_menu().len(),
            Self::Keys => keys::NAMES.len(),
            Self::RelatedKeys(_) => 3,
            Self::Library { rows, .. } => rows.len(),
            Self::Metadata(_) => 16,
            Self::TrackInfo(_) => 7,
            Self::DeliveryInfo(_) => 13,
            Self::Empty => 0,
        };
        u32::try_from(n).unwrap_or(u32::MAX)
    }
}

pub struct LinkSession {
    catalog: Arc<dyn Catalog>,
    menus: HashMap<u8, Menu>,
    /// The player's device number, from its setup message.
    player: u8,
    extended: bool,
    filter: crate::filter::TrackFilter,
    /// Our own, in the setup reply.
    device: u8,
}

impl LinkSession {
    pub fn new(catalog: Arc<dyn Catalog>) -> Self {
        Self { catalog, menus: HashMap::new(), player: 0, extended: true, filter: crate::filter::TrackFilter::default(), device: DEVICE }
    }

    /// The session answering as `device` rather than 17.
    #[must_use]
    pub fn as_device(mut self, device: u8) -> Self {
        self.device = device;
        self
    }

    fn number(message: &Message, index: usize) -> u32 {
        match message.arguments.get(index) {
            Some(Argument::Number(n)) => *n,
            _ => 0,
        }
    }

    fn text(message: &Message, index: usize) -> String {
        match message.arguments.get(index) {
            Some(Argument::String(s)) => s.clone(),
            _ => String::new(),
        }
    }

    /// Answers a menu request: remember the menu, reply with its size.
    fn menu(&mut self, message: &Message, menu: Menu) -> Vec<Message> {
        let count = menu.len();
        self.menus.insert(Self::menu_location(message), menu);
        vec![menu_header(message.transaction, u32::from(message.kind), count)]
    }

    fn library(&mut self, message: &Message, query: Query) -> Vec<Message> {
        let mut rows = self.catalog.list(&query);
        self.catalog.filter_rows(&mut rows, &self.filter);
        self.menu(message, Menu::Library { query, rows })
    }

    fn tracks(&mut self, message: &Message, scope: TrackScope) -> Vec<Message> {
        let sort = Sort::from_id(Self::number(message, 1));
        self.library(message, Query::Tracks { scope, sort })
    }

    fn menu_location(message: &Message) -> u8 {
        Self::number(message, 0).to_be_bytes()[1]
    }

    /// The items for a window of the current menu.
    fn items(&self, location: u8, offset: u32, limit: u32) -> Vec<Item> {
        let offset = offset as usize;
        let limit = limit as usize;
        let window = |all: Vec<Item>| all.into_iter().skip(offset).take(limit).collect::<Vec<_>>();
        match self.menus.get(&location).unwrap_or(&Menu::Empty) {
            Menu::Root => window(root_menu()),
            Menu::SortOptions => window(sort_menu()),
            Menu::Keys => window(
                self.catalog.key_ids().into_iter()
                    .map(|id| Item::named(id, keys::name(id), item_type::KEY))
                    .collect(),
            ),
            // `[distance, key, text]`: the distance is what a later
            // key-tracks request carries.
            Menu::RelatedKeys(key) => window(
                (0..3)
                    .map(|distance| Item {
                        a: distance,
                        id: *key,
                        text: keys::related_text(*key, distance),
                        item_type: item_type::KEY,
                        ..Item::default()
                    })
                    .collect(),
            ),
            Menu::Library { query, rows } => rows
                .iter()
                .skip(offset)
                .take(limit)
                .filter_map(|row| self.item(query, row))
                .collect(),
            Menu::Metadata(details) => window(metadata_rows(details, self.catalog.played(details.row.id), self.catalog.tagged(details.row.id))),
            Menu::TrackInfo(details) => window(track_info_rows(details)),
            Menu::DeliveryInfo(details) => window(delivery_rows(details)),
            Menu::Empty => Vec::new(),
        }
    }

    /// One library row as the item its menu draws it as.
    fn item(&self, query: &Query, row: &Row) -> Option<Item> {
        Some(match (query, row) {
            (Query::Artists(_), Row::Named { id, name }) => Item::named(*id, name, item_type::ARTIST),
            (Query::Albums(_) | Query::ArtistAlbums(_), Row::Named { id, name }) => {
                if *id == 0xffff_ffff {
                    Item::all()
                } else {
                    Item::named_twice(*id, name, item_type::ALBUM)
                }
            }
            (Query::Histories, Row::Named { id, name }) => Item::named(*id, name, item_type::HISTORY),
            (_, Row::Named { id, name }) => Item::named(*id, name, item_type::GENRE),
            (_, Row::List { id, name, folder, position }) => Item::list(*id, name, *folder, *position),
            (_, Row::Date(value)) => {
                if *value == 0xffff_ffff {
                    Item::all()
                } else {
                    Item::date_part(*value)
                }
            }
            (Query::Tracks { scope, .. }, Row::Track { id, position }) => {
                let track = self.catalog.track_row(*id)?;
                let listed = match scope {
                    TrackScope::Artist { .. } | TrackScope::Album(_) | TrackScope::Playlist(_) | TrackScope::TagList => track_flags::LISTED,
                    _ => 0,
                };
                // A history's rows are all played; elsewhere only the tracks
                // a player has loaded this session are, or every row of a
                // playlist greys.
                let played = matches!(scope, TrackScope::History(_)) || self.catalog.played(*id);
                let flags = listed | if played { track_flags::PLAYED } else { 0 } | u32::from(self.catalog.tagged(*id));
                Item::track(&track, flags, *position)
            }
            (_, Row::Track { .. }) => return None,
        })
    }

    /// A blob reply: `[request kind, 0, len, blob]` and, for some, a
    /// trailing number; or `[kind, 0x32, 0]` when there is nothing.
    fn blob(message: &Message, reply: u16, blob: Option<Vec<u8>>, tail: Option<u32>) -> Vec<Message> {
        let mut arguments = vec![Argument::Number(u32::from(message.kind))];
        match blob {
            Some(bytes) if !bytes.is_empty() => {
                arguments.push(Argument::Number(0));
                arguments.push(Argument::Number(u32::try_from(bytes.len()).unwrap_or(u32::MAX)));
                arguments.push(Argument::Blob(bytes));
            }
            _ => {
                arguments.push(Argument::Number(0x32));
                arguments.push(Argument::Number(0));
                arguments.push(Argument::Blob(Vec::new()));
            }
        }
        if let Some(tail) = tail {
            arguments.push(Argument::Number(tail));
        }
        vec![Message::new(message.transaction, reply, arguments)]
    }

    fn analysis(&self, message: &Message, track: u32, what: &Analysis, reply: u16, tail: Option<u32>) -> Vec<Message> {
        Self::blob(message, reply, self.catalog.analysis(track, what), tail)
    }

    /// A menu of one track's fields: metadata, track info or delivery info,
    /// all `[ctx, track_id]` and all opened the same way.
    fn track_menu(&mut self, message: &Message, make: fn(Box<TrackDetails>) -> Menu) -> Vec<Message> {
        let track = Self::number(message, 1);
        match self.catalog.track(track) {
            Some(details) => self.menu(message, make(Box::new(details))),
            None => self.menu(message, Menu::Empty),
        }
    }

    /// The requests that open a menu: a count now, rows on render.
    fn handle_menu(&mut self, message: &Message) -> Vec<Message> {
        match message.kind {
            kind::ROOT_MENU => self.menu(message, Menu::Root),
            kind::SORT_MENU => self.menu(message, Menu::SortOptions),
            kind::KEY_MENU => self.menu(message, Menu::Keys),
            kind::RELATED_KEYS => {
                let key = Self::number(message, 2);
                self.menu(message, Menu::RelatedKeys(key))
            }
            kind::ARTIST_MENU => {
                let sort = Sort::from_id(Self::number(message, 1));
                self.library(message, Query::Artists(sort))
            }
            kind::ALBUM_MENU => {
                let sort = Sort::from_id(Self::number(message, 1));
                self.library(message, Query::Albums(sort))
            }
            kind::ARTIST_ALBUMS => {
                let artist = Self::number(message, 2);
                let mut rows = vec![Row::Named { id: 0xffff_ffff, name: String::new() }];
                rows.extend(self.catalog.list(&Query::ArtistAlbums(artist)));
                self.menu(message, Menu::Library { query: Query::ArtistAlbums(artist), rows })
            }
            kind::ARTIST_ALBUM_TRACKS => {
                let artist = Self::number(message, 2);
                let album = Self::number(message, 3);
                let album = (album != 0xffff_ffff).then_some(album);
                self.tracks(message, TrackScope::Artist { artist, album })
            }
            kind::ALBUM_TRACKS => {
                let album = Self::number(message, 2);
                self.tracks(message, TrackScope::Album(album))
            }
            kind::TRACK_MENU => self.tracks(message, TrackScope::All),
            kind::TAG_LIST => self.tracks(message, TrackScope::TagList),
            kind::KEY_TRACKS => {
                let key = Self::number(message, 2);
                let distance = Self::number(message, 3).min(2);
                self.tracks(message, TrackScope::Key { key, distance })
            }
            kind::PLAYLIST_MENU => {
                let id = Self::number(message, 2);
                if Self::number(message, 3) == 1 {
                    self.library(message, Query::Folder(id))
                } else {
                    self.tracks(message, TrackScope::Playlist(id))
                }
            }
            kind::HISTORY_MENU => self.library(message, Query::Histories),
            kind::HISTORY_TRACKS => {
                let session = Self::number(message, 2);
                self.tracks(message, TrackScope::History(session))
            }
            kind::YEARS => self.library(message, Query::Years),
            kind::MONTHS => {
                let year = Self::number(message, 2);
                let mut rows = vec![Row::Date(0xffff_ffff)];
                rows.extend(self.catalog.list(&Query::Months(year)));
                self.menu(message, Menu::Library { query: Query::Months(year), rows })
            }
            kind::DAYS => {
                let year = Self::number(message, 2);
                let month = Self::number(message, 3);
                if month == 0xffff_ffff {
                    // ⟨ALL⟩ months: the year's tracks come next, so there
                    // is only the ⟨ALL⟩ row to offer.
                    let query = Query::Days { year, month };
                    return self.menu(message, Menu::Library { query, rows: vec![Row::Date(0xffff_ffff)] });
                }
                let mut rows = vec![Row::Date(0xffff_ffff)];
                rows.extend(self.catalog.list(&Query::Days { year, month }));
                self.menu(message, Menu::Library { query: Query::Days { year, month }, rows })
            }
            kind::DATE_TRACKS => {
                let year = Self::number(message, 2);
                let month = Self::number(message, 3);
                let day = Self::number(message, 4);
                let month = (month != 0xffff_ffff).then_some(month);
                let day = (day != 0xffff_ffff).then_some(day);
                self.tracks(message, TrackScope::DateAdded { year, month, day })
            }
            kind::SEARCH | kind::SEARCH_TRACK => {
                let text = Self::text(message, 3);
                self.tracks(message, TrackScope::Search(text))
            }
            kind::METADATA => self.track_menu(message, Menu::Metadata),
            kind::TRACK_INFO => self.track_menu(message, Menu::TrackInfo),
            kind::DELIVERY_INFO => self.track_menu(message, Menu::DeliveryInfo),
            // `3007` after setup, MATCHING, and anything not built: a menu
            // with nothing in it, which a player takes in its stride.
            _ => self.menu(message, Menu::Empty),
        }
    }

    /// The requests answered with a blob.
    fn handle_blob(&mut self, message: &Message) -> Vec<Message> {
        match message.kind {
            // rekordbox's blob carries its own account's KUVO details; ours
            // is zero, a user with nothing to say. Only its presence and
            // length were seen to matter: the player's KUVO ticket waits
            // for this reply, copies the first 32 bytes, and moves on to
            // the delivery info. `[UNKNOWN]` what a KUVO user would put
            // here; the capture is verification/link/kuvo-delivery-20260919.txt.
            kind::USER_INFO => Self::blob(message, kind::USER_INFO_REPLY, Some(vec![0; USER_INFO_LEN]), None),
            kind::ARTWORK => {
                let id = Self::number(message, 1);
                // With the size argument the id is the menu item's own; without
                // it, the artwork field of a title item (`Catalog::item_artwork`).
                let art = if message.arguments.len() > 2 { self.catalog.item_artwork(id) } else { self.catalog.artwork(id) };
                Self::blob(message, kind::ARTWORK_REPLY, art, None)
            }
            kind::WAVEFORM_PREVIEW => {
                let track = Self::number(message, 2);
                self.analysis(message, track, &Analysis::WaveformPreview, kind::WAVEFORM_PREVIEW_REPLY, None)
            }
            kind::BEAT_GRID => {
                let track = Self::number(message, 1);
                self.analysis(message, track, &Analysis::BeatGrid, kind::BEAT_GRID_REPLY, Some(u32::from(u16::from_ne_bytes(self.catalog.grid_offset(track).to_ne_bytes()))))
            }
            kind::CUES => {
                let track = Self::number(message, 1);
                self.analysis(message, track, &Analysis::CueList, kind::CUES_REPLY, None)
            }
            kind::WAVEFORM_DETAIL => {
                let track = Self::number(message, 1);
                self.analysis(message, track, &Analysis::WaveformDetail, kind::WAVEFORM_DETAIL_REPLY, None)
            }
            kind::EXTENDED_CUES => {
                let track = Self::number(message, 1);
                // The trailing number is the cue count; the catalog's blob
                // carries it in its header, which the reply repeats.
                let blob = self.catalog.analysis(track, &Analysis::ExtendedCueList);
                let count = blob.as_ref().map_or(0, |b| extended_cue_count(b));
                Self::blob(message, kind::EXTENDED_CUES_REPLY, blob, Some(count))
            }
            kind::ANLZ_TAG | kind::ANLZ_TAG_2EX => {
                let track = Self::number(message, 1);
                let fourcc = Self::number(message, 2).to_le_bytes();
                let ext = Self::number(message, 3).to_le_bytes();
                let what = Analysis::Tag { fourcc, extension: [ext[0], ext[1], ext[2]] };
                self.analysis(message, track, &what, kind::ANLZ_TAG_REPLY, Some(1))
            }
            _ => Vec::new(),
        }
    }
}

impl Session for LinkSession {
    fn handle(&mut self, message: &Message) -> Vec<Message> {
        let tx = message.transaction;
        match message.kind {
            kind::SETUP => {
                self.player = u8::try_from(Self::number(message, 0)).unwrap_or(0);
                self.extended = message.arguments.len() > 1;
                if self.extended {
                    vec![setup_reply(tx, self.device)]
                } else {
                    vec![menu_header(tx, 0, u32::from(self.device))]
                }
            }
            kind::TEARDOWN => Vec::new(),
            kind::GRID_OFFSET => vec![menu_header(tx, u32::from(message.kind), u32::from(u16::from_ne_bytes(self.catalog.grid_offset(Self::number(message, 1)).to_ne_bytes())))],
            kind::SAVE_GRID_OFFSET => {
                let raw = Self::number(message, 2).to_be_bytes();
                let success = self.catalog.edit(&Edit::GridOffset {
                    track: Self::number(message, 1), offset_ms: i16::from_be_bytes([raw[2], raw[3]]),
                });
                vec![menu_header(tx, u32::from(message.kind), u32::from(!success))]
            }
            kind::FILTER_SWITCH => {
                self.filter.enabled = Self::number(message, 1) != 0;
                vec![menu_header(tx, u32::from(message.kind), 0)]
            }
            kind::FILTER_GET => Self::blob(message, kind::FILTER_REPLY, Some(self.filter.encode()), Some(4)),
            kind::FILTER_SET => {
                let valid = match message.arguments.get(4) {
                    Some(Argument::Blob(bytes)) if bytes.len() == Self::number(message, 3) as usize => self.filter.update(Self::number(message, 1), bytes),
                    _ => false,
                };
                vec![menu_header(tx, u32::from(message.kind), u32::from(!valid))]
            }
            kind::CHANGE_TAG | kind::CLEAR_TAGS | kind::CHANGE_RATING => {
                let track = Self::number(message, 1);
                let value = Self::number(message, 2);
                let edit = match message.kind {
                    kind::CHANGE_TAG if value <= 1 => Some(Edit::Tag { track, add: value == 1 }),
                    kind::CLEAR_TAGS => Some(Edit::ClearTags),
                    kind::CHANGE_RATING if value <= 5 => Some(Edit::Rating { track, stars: u8::try_from(value).unwrap_or(0) }),
                    _ => None,
                };
                let success = edit.is_some_and(|edit| self.catalog.edit(&edit));
                vec![menu_header(tx, u32::from(message.kind), u32::from(!success))]
            }

            kind::RENDER => {
                let offset = Self::number(message, 1);
                let limit = Self::number(message, 2);
                let mut out = vec![Message::new(
                    tx,
                    kind::RENDER_HEADER,
                    vec![Argument::Number(1), Argument::Number(offset)],
                )];
                out.extend(self.items(Self::menu_location(message), offset, limit).iter().map(|item| {
                    let mut row = item.clone();
                    if !self.extended && row.item_type == item_type::TRACK {
                        row.item_type = item_type::TITLE;
                        row.text2.clear();
                    }
                    let mut reply = row.message(tx);
                    if !self.extended {
                        reply.arguments.truncate(12);
                    }
                    reply
                }));
                out.push(if self.extended { menu_footer(tx) } else { Message::new(tx, kind::MENU_FOOTER, vec![]) });
                out
            }
            kind::ITEM_POSITION => {
                let id = Self::number(message, 1);
                let location = Self::menu_location(message);
                let position = match self.menus.get(&location) {
                    Some(Menu::Library { rows, .. }) => rows.iter().position(|row| match row {
                        Row::Named { id: item, .. } | Row::List { id: item, .. } | Row::Track { id: item, .. } | Row::Date(item) => *item == id,
                    }),
                    _ => self.items(location, 0, u32::MAX).iter().position(|item| item.id == id),
                }.and_then(|n| u32::try_from(n).ok()).unwrap_or(u32::MAX);
                vec![menu_header(tx, u32::from(kind::ITEM_POSITION), position)]
            }
            kind::ARTWORK
            | kind::WAVEFORM_PREVIEW
            | kind::BEAT_GRID
            | kind::CUES
            | kind::WAVEFORM_DETAIL
            | kind::EXTENDED_CUES
            | kind::ANLZ_TAG
            | kind::ANLZ_TAG_2EX
            | kind::USER_INFO => self.handle_blob(message),
            _ => self.handle_menu(message),
        }
    }
}

/// The number of cues an extended cue list holds, which the reply repeats as
/// its trailing argument. The blob is the entries concatenated with no count
/// header, each led by its own byte length as a little-endian u32, so the
/// count is recovered by walking them — the way rekordbox's own count matches
/// its blob. Reading a fixed offset instead gave a CDJ a nonsense count (the
/// first entry's own fields) and it faulted allocating for that many cues.
fn extended_cue_count(blob: &[u8]) -> u32 {
    let mut offset = 0;
    let mut count = 0;
    while let [a, b, c, d] = blob.get(offset..offset + 4).unwrap_or(&[]) {
        let entry_len = u32::from_le_bytes([*a, *b, *c, *d]) as usize;
        if entry_len < 4 {
            break;
        }
        offset += entry_len;
        count += 1;
    }
    count
}

/// The sixteen rows of a metadata reply, one per column, in rekordbox's order.
fn metadata_rows(t: &TrackDetails, played: bool, tagged: bool) -> Vec<Item> {
    let key_id = t.row.key;
    vec![
        Item::track(&t.row, (if played { track_flags::PLAYED } else { 0 }) | u32::from(tagged), 0),
        Item::line(1, t.artist_id, &t.artist, item_type::ARTIST),
        Item::line(1, t.album_id, &t.album, item_type::ALBUM),
        Item::line(0, t.duration_s, "", item_type::DURATION),
        Item::line(0, t.row.bpm_x100, "", item_type::TEMPO),
        Item::line(1, key_id, keys::name(key_id), item_type::KEY),
        Item::line(0, t.rating, "", item_type::RATING),
        Item::line(0, t.colour, "", item_type::COLOUR),
        Item::line(0, t.genre_id, &t.genre, item_type::GENRE),
        Item::line(1, t.row.id, &t.date_added, item_type::DATE_ADDED),
        Item::line(0, t.row.id, &t.row.comment, item_type::COMMENT),
        Item::line(0, t.year, "", item_type::YEAR),
        Item::line(0, t.bit_rate_kbps, "", item_type::BIT_RATE),
        Item::line(0, t.label_id, &t.label, item_type::LABEL),
        Item::line(0, 0, &t.original_artist, item_type::ORIGINAL_ARTIST),
        Item::line(0, 0, &t.remixer, item_type::REMIXER),
    ]
}

/// The thirteen rows of a delivery-info reply, in rekordbox's order (captured
/// from 7.2.11 answering a CDJ-3000 that had just loaded a track from it).
/// The firmware's KUVO ticket reads them by type: the texts of ARTIST, ALBUM,
/// GENRE, LABEL, COMMENT, 0x36 and 0x37; the ids of DURATION, TEMPO, KEY and
/// `FILE_TYPE`; from the `TITLE` row its text, first slot and ninth slot; and
/// from the closing 0x4f row its first slot, ninth slot and second text.
fn delivery_rows(t: &TrackDetails) -> Vec<Item> {
    let key_id = t.row.key;
    vec![
        Item::line(0, 0, "", item_type::DELIVERY_TEXT_36),
        Item::line(0, t.artist_id, &t.artist, item_type::ARTIST),
        Item::line(0, key_id, "", item_type::KEY),
        Item::line(0, t.duration_s, "", item_type::DURATION),
        Item {
            a: t.row.id,
            id: t.row.id,
            text: t.row.title.clone(),
            item_type: item_type::TITLE,
            flags: track_flags::LISTED,
            c: t.row.id,
            e: 0x100,
            f: t.row.bpm_x100,
            ..Item::default()
        },
        Item::line(0, t.row.id, &t.row.comment, item_type::COMMENT),
        Item::line(0, t.album_id, &t.album, item_type::ALBUM),
        Item::line(0, t.row.bpm_x100, "", item_type::TEMPO),
        Item::line(0, t.row.id, "", item_type::DELIVERY_TEXT_37),
        Item::line(0, t.label_id, &t.label, item_type::LABEL),
        Item::line(0, t.file_type, "", item_type::FILE_TYPE),
        Item::line(0, t.genre_id, &t.genre, item_type::GENRE),
        Item { a: t.row.id, item_type: item_type::DELIVERY_ID, c: 1, ..Item::default() },
    ]
}

/// The seven rows of a track-info reply.
fn track_info_rows(t: &TrackDetails) -> Vec<Item> {
    let key_id = t.row.key;
    vec![
        // rekordbox puts the file's copyright comment here; we have no such
        // field, so the row is blank but shaped the same.
        Item {
            a: 1,
            id: 1,
            item_type: item_type::TRACK,
            e: 0x100,
            key: key_id,
            art: if t.row.artwork == 0 { 1 } else { t.row.artwork },
            text3: t.row.key_name.clone(),
            f: t.row.bpm_x100,
            ..Item::default()
        },
        Item::line(0, t.duration_s, "", item_type::DURATION),
        Item::line(0, t.row.bpm_x100, "", item_type::TEMPO),
        Item::line(0, t.row.id, &t.row.comment, item_type::COMMENT),
        Item::line(t.file_size, t.row.id, &t.path, item_type::PATH),
        Item::line(0, 1, "", item_type::INFO_UNKNOWN),
        Item::line(0, key_id, keys::name(key_id), item_type::KEY),
    ]
}
