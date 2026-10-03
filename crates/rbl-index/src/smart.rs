//! Intelligent playlists: the rule in `djmdPlaylist.SmartList`, parsed and
//! evaluated over the columns.
//!
//! rekordbox stores the rule as a small XML document:
//!
//! ```xml
//! <NODE Id="123" LogicalOperator="1" AutomaticUpdate="1">
//!   <CONDITION PropertyName="genre" Operator="8" ValueUnit="" ValueLeft="House" ValueRight=""/>
//!   <CONDITION PropertyName="bpm" Operator="5" ValueUnit="" ValueLeft="120" ValueRight="130"/>
//! </NODE>
//! ```
//!
//! `LogicalOperator` 1 is "all of the following", 2 "any of". The operators
//! are numbered as rekordbox's picker lists them: equal, not equal, greater,
//! less, in range, in the last, not in the last, contains, does not contain,
//! starts with, ends with. The property names are rekordbox's own internal
//! ones (`name` is the title, `counter` the play count, `grouping` the
//! colour, `producer` the composer, `stockDate` the date added).
//!
//! Read with `rbl_core::xml`'s scanner: the document is a flat handful of
//! elements with quoted attributes, and a rule that does not parse is
//! simply a playlist with nothing in it.
//!
//! The format is what rekordbox 6 and 7 write, as documented by the
//! community (pyrekordbox's `smartlist` module) rather than measured against
//! this library: there was no rekordbox library on the machine that wrote
//! this, so the value conventions marked `[ASSUME]` below want checking
//! against a recorded rule the first time one is to hand.

use rbl_core::xml::{attribute, tags, Tag};

use crate::strings::fold_smart;
use crate::{Library, Row, NO_ID};

/// How a group combines its conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Logic {
    /// Every condition must hold (`LogicalOperator="1"`).
    All,
    /// Any one condition suffices (`LogicalOperator="2"`).
    Any,
}

/// The comparison a condition makes, numbered as rekordbox numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Equal,
    NotEqual,
    Greater,
    Less,
    InRange,
    InLast,
    NotInLast,
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
}

impl Operator {
    /// The number rekordbox stores.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Equal => "1",
            Self::NotEqual => "2",
            Self::Greater => "3",
            Self::Less => "4",
            Self::InRange => "5",
            Self::InLast => "6",
            Self::NotInLast => "7",
            Self::Contains => "8",
            Self::NotContains => "9",
            Self::StartsWith => "10",
            Self::EndsWith => "11",
        }
    }

    /// The operator with this number.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Some(match code.trim() {
            "1" => Self::Equal,
            "2" => Self::NotEqual,
            "3" => Self::Greater,
            "4" => Self::Less,
            "5" => Self::InRange,
            "6" => Self::InLast,
            "7" => Self::NotInLast,
            "8" => Self::Contains,
            "9" => Self::NotContains,
            "10" => Self::StartsWith,
            "11" => Self::EndsWith,
            _ => return None,
        })
    }
}

/// What a condition looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Property {
    Artist,
    Album,
    AlbumArtist,
    OriginalArtist,
    Composer,
    Remixer,
    MixName,
    Genre,
    Label,
    Key,
    Title,
    Comment,
    FileName,
    Bpm,
    Rating,
    Color,
    PlayCount,
    Duration,
    Year,
    DateAdded,
    DateCreated,
    DateReleased,
    /// A property the index does not hold. Never matches.
    Unsupported,
}

impl Property {
    /// rekordbox's internal name for the property, the one in the XML.
    /// `Unsupported` has none; it is written as an empty name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Artist => "artist",
            Self::Album => "album",
            Self::AlbumArtist => "albumArtist",
            Self::OriginalArtist => "originalArtist",
            Self::Composer => "producer",
            Self::Remixer => "remixedBy",
            Self::MixName => "mixName",
            Self::Genre => "genre",
            Self::Label => "label",
            Self::Key => "key",
            Self::Title => "name",
            Self::Comment => "comments",
            Self::FileName => "fileName",
            Self::Bpm => "bpm",
            Self::Rating => "rating",
            Self::Color => "grouping",
            Self::PlayCount => "counter",
            Self::Duration => "duration",
            Self::Year => "year",
            Self::DateAdded => "stockDate",
            Self::DateCreated => "dateCreated",
            Self::DateReleased => "dateReleased",
            Self::Unsupported => "",
        }
    }

    /// The property rekordbox calls `name` in the XML.
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name.trim() {
            "artist" => Self::Artist,
            "album" => Self::Album,
            "albumArtist" => Self::AlbumArtist,
            "originalArtist" => Self::OriginalArtist,
            "producer" => Self::Composer,
            "remixedBy" => Self::Remixer,
            "mixName" => Self::MixName,
            "genre" => Self::Genre,
            "label" => Self::Label,
            "key" => Self::Key,
            "name" => Self::Title,
            "comments" => Self::Comment,
            "fileName" => Self::FileName,
            "bpm" => Self::Bpm,
            "rating" => Self::Rating,
            "grouping" => Self::Color,
            "counter" => Self::PlayCount,
            "duration" => Self::Duration,
            "year" => Self::Year,
            "stockDate" => Self::DateAdded,
            "dateCreated" => Self::DateCreated,
            "dateReleased" => Self::DateReleased,
            _ => Self::Unsupported,
        }
    }
}

/// One line of the rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    pub property: Property,
    pub operator: Operator,
    pub left: String,
    pub right: String,
    /// The unit of an "in the last" count: `day`, `week`, `month` or `year`.
    pub unit: String,
}

/// A group of conditions, or of groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub logic: Logic,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Condition(Condition),
    Group(Group),
}

/// A parsed rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmartRule {
    pub root: Group,
}

impl SmartRule {
    /// Parses the XML. `None` when there is no `NODE` in it at all.
    #[must_use]
    pub fn parse(xml: &str) -> Option<Self> {
        let mut stack: Vec<Group> = Vec::new();
        let mut root: Option<Group> = None;
        for tag in tags(xml) {
            match tag {
                Tag::Open { name, attributes, closed } if name.eq_ignore_ascii_case("NODE") => {
                    let logic = if attribute(&attributes, "LogicalOperator").trim() == "2" {
                        Logic::Any
                    } else {
                        Logic::All
                    };
                    let group = Group { logic, items: Vec::new() };
                    if closed {
                        close_group(&mut stack, &mut root, group);
                    } else {
                        stack.push(group);
                    }
                }
                Tag::Open { name, attributes, .. } if name.eq_ignore_ascii_case("CONDITION") => {
                    let Some(operator) = Operator::from_code(&attribute(&attributes, "Operator")) else {
                        continue;
                    };
                    let condition = Condition {
                        property: Property::from_name(&attribute(&attributes, "PropertyName")),
                        operator,
                        left: attribute(&attributes, "ValueLeft"),
                        right: attribute(&attributes, "ValueRight"),
                        unit: attribute(&attributes, "ValueUnit"),
                    };
                    if let Some(group) = stack.last_mut() {
                        group.items.push(Item::Condition(condition));
                    }
                }
                Tag::Close { name } if name.eq_ignore_ascii_case("NODE") => {
                    if let Some(group) = stack.pop() {
                        close_group(&mut stack, &mut root, group);
                    }
                }
                Tag::Open { .. } | Tag::Close { .. } => {}
            }
        }
        // An unterminated document keeps what was read.
        while let Some(group) = stack.pop() {
            close_group(&mut stack, &mut root, group);
        }
        root.map(|root| Self { root })
    }

    /// The rule as `djmdPlaylist.SmartList` holds it, for the playlist with
    /// `id`: the root `NODE` names the playlist, `AutomaticUpdate` is on,
    /// and each condition is one `CONDITION`. The shape is the one parsed
    /// above, written back without whitespace between the elements
    /// [ASSUME: no rekordbox-written rule has been captured to copy its
    /// spacing; the reader here and rekordbox's both ignore it].
    #[must_use]
    pub fn to_xml(&self, id: u64) -> String {
        use std::fmt::Write as _;
        fn write_group(out: &mut String, group: &Group, id: Option<u64>) {
            let logic = match group.logic {
                Logic::All => "1",
                Logic::Any => "2",
            };
            match id {
                Some(id) => {
                    let _ = write!(out, "<NODE Id=\"{id}\" LogicalOperator=\"{logic}\" AutomaticUpdate=\"1\">");
                }
                None => {
                    let _ = write!(out, "<NODE LogicalOperator=\"{logic}\">");
                }
            }
            for item in &group.items {
                match item {
                    Item::Condition(c) => {
                        let _ = write!(
                            out,
                            "<CONDITION PropertyName=\"{}\" Operator=\"{}\" ValueUnit=\"{}\" ValueLeft=\"{}\" ValueRight=\"{}\"/>",
                            c.property.name(),
                            c.operator.code(),
                            rbl_core::xml::escape(&c.unit),
                            rbl_core::xml::escape(&c.left),
                            rbl_core::xml::escape(&c.right),
                        );
                    }
                    Item::Group(g) => write_group(out, g, None),
                }
            }
            out.push_str("</NODE>");
        }
        let mut out = String::with_capacity(128 + self.root.items.len() * 96);
        write_group(&mut out, &self.root, Some(id));
        out
    }

    /// How many conditions name something the index cannot answer.
    #[must_use]
    pub fn unsupported(&self) -> usize {
        fn count(group: &Group) -> usize {
            group
                .items
                .iter()
                .map(|item| match item {
                    Item::Condition(c) => usize::from(c.property == Property::Unsupported),
                    Item::Group(g) => count(g),
                })
                .sum()
        }
        count(&self.root)
    }

    /// The rows of the library the rule admits, in collection order.
    #[must_use]
    pub fn evaluate(&self, library: &Library) -> Vec<Row> {
        self.evaluate_on(library, &today())
    }

    /// As [`evaluate`](Self::evaluate), with the date the relative
    /// conditions count back from.
    #[must_use]
    pub fn evaluate_on(&self, library: &Library, today: &Date) -> Vec<Row> {
        let compiled = CompiledGroup::from(&self.root, library, today);
        let count = u32::try_from(library.len()).unwrap_or(u32::MAX);
        (0..count).filter(|&row| compiled.matches(library, row)).collect()
    }
}

fn close_group(stack: &mut [Group], root: &mut Option<Group>, group: Group) {
    match stack.last_mut() {
        Some(parent) => parent.items.push(Item::Group(group)),
        None => {
            if root.is_none() {
                *root = Some(group);
            }
        }
    }
}

// ---------------------------------------------------------------- evaluation

/// A rule with its values parsed once, so the row loop compares integers and
/// folded strings without allocating.
struct CompiledGroup {
    logic: Logic,
    items: Vec<CompiledItem>,
}

enum CompiledItem {
    Condition(CompiledCondition),
    Group(CompiledGroup),
}

struct CompiledCondition {
    property: Property,
    operator: Operator,
    /// The text value, folded, for the string properties.
    text: String,
    /// The numeric bounds, for the number and date properties: the value,
    /// or the two ends of a range, or the earliest day of "in the last".
    low: i64,
    high: i64,
}

impl CompiledGroup {
    fn from(group: &Group, library: &Library, today: &Date) -> Self {
        Self {
            logic: group.logic,
            items: group
                .items
                .iter()
                .map(|item| match item {
                    Item::Condition(c) => CompiledItem::Condition(CompiledCondition::from(c, library, today)),
                    Item::Group(g) => CompiledItem::Group(Self::from(g, library, today)),
                })
                .collect(),
        }
    }

    fn matches(&self, library: &Library, row: Row) -> bool {
        // A group with nothing in it admits everything under "all", which is
        // what an empty rule shows in rekordbox: the whole collection.
        match self.logic {
            Logic::All => self.items.iter().all(|item| item.matches(library, row)),
            Logic::Any => self.items.iter().any(|item| item.matches(library, row)),
        }
    }
}

impl CompiledItem {
    fn matches(&self, library: &Library, row: Row) -> bool {
        match self {
            Self::Condition(c) => c.matches(library, row),
            Self::Group(g) => g.matches(library, row),
        }
    }
}

impl CompiledCondition {
    fn from(condition: &Condition, library: &Library, today: &Date) -> Self {
        let (low, high) = match condition.property {
            Property::Bpm => (whole(&condition.left), whole(&condition.right)),
            Property::Duration => (seconds(&condition.left), seconds(&condition.right)),
            Property::Rating | Property::PlayCount | Property::Year => {
                (whole(&condition.left), whole(&condition.right))
            }
            Property::Color => (color_id(&condition.left), color_id(&condition.right)),
            Property::DateAdded | Property::DateCreated | Property::DateReleased => {
                match condition.operator {
                    Operator::InLast | Operator::NotInLast => {
                        let back = whole(&condition.left).max(0);
                        (today.minus(back, &condition.unit).days(), today.days())
                    }
                    _ => (
                        Date::parse(&condition.left).map_or(i64::MIN, Date::days),
                        Date::parse(&condition.right).map_or(i64::MAX, Date::days),
                    ),
                }
            }
            _ => (0, 0),
        };
        let text = match condition.property {
            // A key is picked from the library's own names, so the folded
            // name is what to compare [ASSUME: rekordbox writes the name,
            // not the `djmdKey` id, into `ValueLeft`].
            Property::Key => fold_smart(condition.left.trim()),
            _ => fold_smart(&condition.left),
        };
        let _ = library;
        Self {
            property: condition.property,
            operator: condition.operator,
            text,
            low,
            high,
        }
    }

    #[inline]
    fn matches(&self, lib: &Library, row: Row) -> bool {
        let index = row as usize;
        match self.property {
            Property::Artist => self.text_matches(
                lib.artists
                    .name(lib.artist.get(index).copied().unwrap_or(NO_ID)),
            ),
            Property::Album => self.text_matches(
                lib.albums
                    .name(lib.album.get(index).copied().unwrap_or(NO_ID)),
            ),
            Property::AlbumArtist => self.text_matches(lib.search_extra[1].get(index)),
            Property::OriginalArtist => self.text_matches(lib.search_extra[3].get(index)),
            Property::Composer => self.text_matches(lib.search_extra[0].get(index)),
            Property::Remixer => self.text_matches(lib.search_extra[2].get(index)),
            Property::MixName => self.text_matches(lib.search_extra[4].get(index)),
            Property::Genre => self.text_matches(
                lib.genres
                    .name(lib.genre.get(index).copied().unwrap_or(NO_ID)),
            ),
            Property::Label => self.text_matches(
                lib.labels
                    .name(lib.label.get(index).copied().unwrap_or(NO_ID)),
            ),
            Property::Key => {
                self.text_matches(lib.keys.name(lib.key.get(index).copied().unwrap_or(NO_ID)))
            }
            Property::Title => self.text_matches(lib.title.get(index)),
            // Not folded ahead of time: comments and file names are searched
            // by the query through the haystack, not compared on their own,
            // so a rule on them folds per row. A few milliseconds over the
            // whole library, once per open, not per keystroke.
            Property::Comment => self.text_matches(lib.comment.get(index)),
            Property::FileName => self.text_matches(lib.file_name.get(index)),
            Property::Bpm => {
                self.number_matches(i64::from(lib.bpm_x100.get(index).copied().unwrap_or(0)))
            }
            Property::Rating => {
                self.number_matches(i64::from(lib.rating.get(index).copied().unwrap_or(0)))
            }
            Property::Color => {
                self.number_matches(i64::from(lib.color.get(index).copied().unwrap_or(0)))
            }
            Property::PlayCount => {
                self.number_matches(i64::from(lib.play_count.get(index).copied().unwrap_or(0)))
            }
            Property::Duration => {
                self.number_matches(i64::from(lib.length_sec.get(index).copied().unwrap_or(0)))
            }
            Property::Year => {
                self.number_matches(i64::from(lib.year.get(index).copied().unwrap_or(0)))
            }
            Property::DateAdded | Property::DateCreated => {
                self.date_matches(lib.date_added.get(index))
            }
            Property::DateReleased => self.date_matches(lib.release_date.get(index)),
            Property::Unsupported => false,
        }
    }

    fn text_matches(&self, text: &str) -> bool {
        let folded = fold_smart(text);
        let folded = folded.as_str();
        let wanted = self.text.as_str();
        match self.operator {
            Operator::Equal => folded == wanted,
            Operator::NotEqual => folded != wanted,
            Operator::Contains => {
                !folded.is_empty() && !wanted.is_empty() && folded.contains(wanted)
            }
            Operator::NotContains => {
                !folded.is_empty() && !wanted.is_empty() && !folded.contains(wanted)
            }
            Operator::StartsWith => {
                !folded.is_empty() && !wanted.is_empty() && folded.starts_with(wanted)
            }
            Operator::EndsWith => {
                !folded.is_empty() && !wanted.is_empty() && folded.ends_with(wanted)
            }
            // Ordering a name makes no sense; rekordbox does not offer it.
            Operator::Greater
            | Operator::Less
            | Operator::InRange
            | Operator::InLast
            | Operator::NotInLast => false,
        }
    }

    fn number_matches(&self, value: i64) -> bool {
        match self.operator {
            Operator::Equal => value == self.low,
            Operator::NotEqual => value != self.low,
            Operator::Greater => value > self.low,
            Operator::Less => value < self.low,
            Operator::InRange => value >= self.low && value <= self.high,
            Operator::InLast
            | Operator::NotInLast
            | Operator::Contains
            | Operator::NotContains
            | Operator::StartsWith
            | Operator::EndsWith => false,
        }
    }

    fn date_matches(&self, text: &str) -> bool {
        let Some(date) = Date::parse(text) else {
            // A track with no date is outside every window, and equal to
            // nothing.
            return matches!(self.operator, Operator::NotEqual | Operator::NotInLast);
        };
        let days = date.days();
        match self.operator {
            Operator::Equal => days == self.low,
            Operator::NotEqual => days != self.low,
            Operator::Greater => days > self.low,
            Operator::Less => days < self.low,
            Operator::InRange | Operator::InLast => days >= self.low && days <= self.high,
            Operator::NotInLast => days < self.low || days > self.high,
            Operator::Contains | Operator::NotContains | Operator::StartsWith | Operator::EndsWith => false,
        }
    }
}

/// A duration in whole seconds, from `300` or `5:00`.
fn seconds(text: &str) -> i64 {
    let text = text.trim();
    if let Some((minutes, secs)) = text.split_once(':') {
        return whole(minutes) * 60 + whole(secs);
    }
    whole(text)
}

/// A numeric rule value, truncated and clamped to rekordbox's signed range.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the clamp defines the conversion range"
)]
fn whole(text: &str) -> i64 {
    let value = text.trim().parse::<f64>().unwrap_or(0.0);
    (value.trunc() as i64).clamp(i64::from(i32::MIN), i64::from(i32::MAX))
}

/// A colour by its `ColorID`, or by name for a rule written with one.
fn color_id(text: &str) -> i64 {
    let text = text.trim();
    if let Ok(id) = text.parse::<i64>() {
        return id;
    }
    crate::filter::COLOR_NAMES
        .iter()
        .position(|name| name.eq_ignore_ascii_case(text))
        .map_or(0, |i| i64::try_from(i).unwrap_or(0) + 1)
}

/// A calendar day, for the date conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    pub year: i64,
    pub month: i64,
    pub day: i64,
}

impl Date {
    /// The first ten characters of a rekordbox date: `YYYY-MM-DD`, whatever
    /// follows.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let mut parts = text.get(..10)?.split('-');
        let year = parts.next()?.parse().ok()?;
        let month = parts.next()?.parse().ok()?;
        let day = parts.next()?.parse().ok()?;
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return None;
        }
        Some(Self { year, month, day })
    }

    /// Days since 1970-01-01, for comparing.
    #[must_use]
    pub fn days(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    /// This date `count` units earlier. Months and years step the calendar,
    /// clamping the day to the month's length, as rekordbox's "in the last
    /// 3 months" means the same day three months back.
    #[must_use]
    pub fn minus(self, count: i64, unit: &str) -> Self {
        let unit = unit.trim().to_ascii_lowercase();
        let unit = unit.trim_end_matches('s');
        match unit {
            "week" => Self::from_days(self.days() - count * 7),
            "month" => self.months_back(count),
            "year" => self.months_back(count * 12),
            // Days, and the unit nobody has seen, which is read as days
            // rather than as no window at all.
            _ => Self::from_days(self.days() - count),
        }
    }

    fn months_back(self, count: i64) -> Self {
        let total = self.year * 12 + (self.month - 1) - count;
        let year = total.div_euclid(12);
        let month = total.rem_euclid(12) + 1;
        Self { year, month, day: self.day.min(days_in_month(year, month)) }
    }

    fn from_days(days: i64) -> Self {
        let (year, month, day) = civil_from_days(days);
        Self { year, month, day }
    }
}

/// The machine's date today, for the relative conditions.
pub(crate) fn today() -> Date {
    Date::parse(&rbl_core::time::local_date()).unwrap_or(Date { year: 1970, month: 1, day: 1 })
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

/// Hinnant's days-from-civil.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Hinnant's civil-from-days.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_rule_parses_into_its_conditions() {
        let rule = SmartRule::parse(
            r#"<NODE Id="1" LogicalOperator="2" AutomaticUpdate="1">
                 <CONDITION PropertyName="genre" Operator="8" ValueUnit="" ValueLeft="Tech &amp; House" ValueRight=""/>
                 <CONDITION PropertyName="bpm" Operator="5" ValueUnit="" ValueLeft="120" ValueRight="130"/>
               </NODE>"#,
        )
        .unwrap();
        assert_eq!(rule.root.logic, Logic::Any);
        assert_eq!(rule.root.items.len(), 2);
        let Item::Condition(first) = &rule.root.items[0] else { panic!() };
        assert_eq!(first.property, Property::Genre);
        assert_eq!(first.operator, Operator::Contains);
        assert_eq!(first.left, "Tech & House");
        assert_eq!(rule.unsupported(), 0);
    }

    #[test]
    fn a_nested_group_and_an_unknown_property_are_kept() {
        let rule = SmartRule::parse(
            r#"<?xml version="1.0"?><NODE LogicalOperator="1"><NODE LogicalOperator="2">
               <CONDITION PropertyName="myTag" Operator="1" ValueLeft="7" ValueRight=""/>
               </NODE><CONDITION PropertyName="rating" Operator="3" ValueLeft="3" ValueRight=""/></NODE>"#,
        )
        .unwrap();
        assert_eq!(rule.root.items.len(), 2);
        assert!(matches!(rule.root.items[0], Item::Group(_)));
        assert_eq!(rule.unsupported(), 1);
        assert!(SmartRule::parse("").is_none());
        assert!(SmartRule::parse("<NODE/>").is_some());
    }

    #[test]
    fn a_rule_written_out_reads_back_the_same() {
        let rule = SmartRule {
            root: Group {
                logic: Logic::Any,
                items: vec![
                    Item::Condition(Condition {
                        property: Property::Genre,
                        operator: Operator::Contains,
                        left: "Tech & \"House\"".to_owned(),
                        right: String::new(),
                        unit: String::new(),
                    }),
                    Item::Group(Group {
                        logic: Logic::All,
                        items: vec![Item::Condition(Condition {
                            property: Property::DateAdded,
                            operator: Operator::InLast,
                            left: "30".to_owned(),
                            right: String::new(),
                            unit: "day".to_owned(),
                        })],
                    }),
                ],
            },
        };
        let xml = rule.to_xml(4_290_236_987);
        assert!(xml.starts_with("<NODE Id=\"4290236987\" LogicalOperator=\"2\" AutomaticUpdate=\"1\"><CONDITION PropertyName=\"genre\" Operator=\"8\""));
        assert_eq!(SmartRule::parse(&xml).unwrap(), rule);
    }

    #[test]
    fn dates_step_back_by_the_calendar() {
        let d = Date::parse("2026-03-31 10:00:00").unwrap();
        assert_eq!(d.minus(1, "month"), Date { year: 2026, month: 2, day: 28 });
        assert_eq!(d.minus(2, "weeks"), Date { year: 2026, month: 3, day: 17 });
        assert_eq!(d.minus(1, "year"), Date { year: 2025, month: 3, day: 31 });
        assert_eq!(d.minus(31, "day"), Date { year: 2026, month: 2, day: 28 });
        assert_eq!(Date::parse("2024-02-29").unwrap().days(), 19_782);
        assert!(Date::parse("not a date").is_none());
    }

    #[test]
    fn values_are_read_the_way_rekordbox_writes_them() {
        assert_eq!(whole("128"), 128);
        assert_eq!(whole("128.5"), 128);
        assert_eq!(whole("12800"), 12_800);
        assert_eq!(whole("0.5"), 0);
        assert_eq!(whole("2147483648"), i64::from(i32::MAX));
        assert_eq!(seconds("5:30"), 330);
        assert_eq!(seconds("330"), 330);
        assert_eq!(color_id("Aqua"), 6);
        assert_eq!(color_id("3"), 3);
        assert_eq!(rbl_core::xml::unescape("a &lt;b&gt; &#39;c&#x27; &unknown; &"), "a <b> 'c' &unknown; &");
    }
}
