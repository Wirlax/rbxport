//! Packed string column.
//!
//! 38k tracks means ~150k short strings. Storing them as `Vec<String>` costs an
//! allocation and 24 bytes of header each; packing them into one arena with
//! (offset, len) pairs cuts that to 8 bytes and keeps them contiguous, which is
//! what makes a full-column scan cache-friendly.

/// Byte arena plus offsets. Indices are row order.
#[derive(Debug, Default, Clone)]
pub struct StrColumn {
    bytes: Vec<u8>,
    spans: Vec<(u32, u32)>,
}

impl StrColumn {
    pub fn with_capacity(rows: usize, bytes: usize) -> Self {
        Self { bytes: Vec::with_capacity(bytes), spans: Vec::with_capacity(rows) }
    }

    pub fn push(&mut self, s: &str) {
        let start = u32::try_from(self.bytes.len()).unwrap_or(u32::MAX);
        self.bytes.extend_from_slice(s.as_bytes());
        let len = u32::try_from(s.len()).unwrap_or(0);
        self.spans.push((start, len));
    }

    #[inline]
    pub fn get(&self, row: usize) -> &str {
        let Some(&(start, len)) = self.spans.get(row) else { return "" };
        let (a, b) = (start as usize, start as usize + len as usize);
        self.bytes.get(a..b).map_or("", |s| std::str::from_utf8(s).unwrap_or(""))
    }

    pub fn len(&self) -> usize {
        self.spans.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// Bytes held, for the memory budget.
    pub fn heap_bytes(&self) -> usize {
        self.bytes.capacity() + self.spans.capacity() * std::mem::size_of::<(u32, u32)>()
    }
}

/// Interning table for lookup columns (artist, album, genre, label, key).
///
/// rekordbox already normalises these into their own tables, so a row holds a
/// `u32` id and the name is stored once.
#[derive(Debug, Default, Clone)]
pub struct Interner {
    names: StrColumn,
    /// Lowercased for sorting and search, in the same row order.
    folded: StrColumn,
}

impl Interner {
    pub fn push(&mut self, name: &str) -> u32 {
        let id = u32::try_from(self.names.len()).unwrap_or(u32::MAX);
        self.names.push(name);
        self.folded.push(&fold(name));
        id
    }

    #[inline]
    pub fn name(&self, id: u32) -> &str {
        if id == u32::MAX { "" } else { self.names.get(id as usize) }
    }

    #[inline]
    pub fn folded(&self, id: u32) -> &str {
        if id == u32::MAX { "" } else { self.folded.get(id as usize) }
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn heap_bytes(&self) -> usize {
        self.names.heap_bytes() + self.folded.heap_bytes()
    }
}

/// Case- and accent-insensitive folding used for both sorting and search.
///
/// Deliberately ASCII-fast: the common case is Latin text, and we only need a
/// consistent ordering, not a locale-correct collation.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            'a'..='z' | '0'..='9' | ' ' => out.push(ch),
            'A'..='Z' => out.push(ch.to_ascii_lowercase()),
            _ => {
                for folded in deaccent(ch) {
                    out.push(*folded);
                }
            }
        }
    }
    out
}

/// Maps the accented Latin-1/Latin-A range onto ASCII; anything else is kept
/// lowercased so non-Latin titles still sort and match consistently.
#[allow(clippy::match_same_arms, reason = "one arm per vowel group reads better than a merged arm")]
fn deaccent(ch: char) -> &'static [char] {
    match ch {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => &['a'],
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => &['e'],
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => &['i'],
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => &['o'],
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => &['u'],
        'ñ' | 'Ñ' => &['n'],
        'ç' | 'Ç' => &['c'],
        'ß' => &['s', 's'],
        'æ' | 'Æ' => &['a', 'e'],
        'ø' | 'Ø' => &['o'],
        _ => &[],
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn packs_and_returns_strings() {
        let mut c = StrColumn::default();
        c.push("Airplane Mode");
        c.push("");
        c.push("Ébano");
        assert_eq!(c.get(0), "Airplane Mode");
        assert_eq!(c.get(1), "");
        assert_eq!(c.get(2), "Ébano");
        assert_eq!(c.get(99), "");
    }

    #[test]
    fn folds_case_and_accents() {
        assert_eq!(fold("Björk"), "bjork");
        assert_eq!(fold("DÉJÀ VU"), "deja vu");
        assert_eq!(fold("Straße"), "strasse");
        assert_eq!(fold("Tiësto"), "tiesto");
    }

    #[test]
    fn folding_drops_punctuation_so_search_ignores_it() {
        assert_eq!(fold("Wh0 - House (Remix)"), "wh0  house remix");
    }

    #[test]
    fn interner_returns_empty_for_the_null_id() {
        let mut i = Interner::default();
        let a = i.push("ARTBAT");
        assert_eq!(i.name(a), "ARTBAT");
        assert_eq!(i.name(u32::MAX), "");
        assert_eq!(i.folded(a), "artbat");
    }
}
