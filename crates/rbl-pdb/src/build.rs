//! Minimal `DeviceSQL` page builder.
//!
//! Enough to construct valid pages for tests today, and the foundation of the
//! export writer. Laying a page out is the fiddly part of the format: rows go
//! into a heap growing forward from the page header while their offsets go into
//! an index growing *backwards* from the end of the page, sixteen to a group,
//! each group preceded by a presence bitmap.

/// Bytes of page header before the heap starts.
pub const PAGE_HEADER_LEN: usize = 0x28;
/// Bytes each row group occupies at the end of a page.
pub const ROW_GROUP_LEN: usize = 0x24;

/// Encodes a string in the short-ASCII `DeviceSQL` form.
///
/// The length byte is "incremented, doubled, and incremented again", which is
/// why a naive `len` byte produces strings that read as garbage.
pub fn short_ascii(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mangled = ((bytes.len() + 1) * 2 + 1).min(0xff);
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.push(u8::try_from(mangled).unwrap_or(0xff));
    out.extend_from_slice(bytes);
    out
}

/// Encodes a string in the long UTF-16LE form, for text ASCII cannot carry.
pub fn long_utf16le(text: &str) -> Vec<u8> {
    let units: Vec<u16> = text.encode_utf16().collect();
    // The length counts the 4-byte header and the two trailing NUL bytes.
    let len = 4 + units.len() * 2 + 2;
    let mut out = Vec::with_capacity(len);
    out.push(0x90);
    out.extend_from_slice(&u16::try_from(len).unwrap_or(u16::MAX).to_le_bytes());
    out.push(0x00);
    for unit in units {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out.extend_from_slice(&[0, 0]);
    out
}

/// Picks the encoding rekordbox would: ASCII when it fits, UTF-16 otherwise.
pub fn device_sql_string(text: &str) -> Vec<u8> {
    if text.is_ascii() && text.len() < 0x7e {
        short_ascii(text)
    } else {
        long_utf16le(text)
    }
}

/// Builds one page of rows.
pub struct PageBuilder {
    page_size: usize,
    page_index: u32,
    page_type: u32,
    next_page: u32,
    heap: Vec<u8>,
    row_offsets: Vec<u16>,
}

impl PageBuilder {
    pub fn new(page_size: usize, page_index: u32, page_type: u32, next_page: u32) -> Self {
        Self {
            page_size,
            page_index,
            page_type,
            next_page,
            heap: Vec::new(),
            row_offsets: Vec::new(),
        }
    }

    /// Appends a row, returning its offset within the heap.
    ///
    /// Rows are aligned to four bytes, as rekordbox's own files are.
    pub fn push_row(&mut self, row: &[u8]) -> u16 {
        while self.heap.len() % 4 != 0 {
            self.heap.push(0);
        }
        let offset = u16::try_from(self.heap.len()).unwrap_or(u16::MAX);
        self.heap.extend_from_slice(row);
        self.row_offsets.push(offset);
        offset
    }

    /// Bytes still available for rows, accounting for the index this page needs.
    pub fn free_space(&self) -> usize {
        let groups = self.row_offsets.len().div_ceil(16).max(1);
        let index = groups * ROW_GROUP_LEN;
        self.page_size
            .saturating_sub(PAGE_HEADER_LEN)
            .saturating_sub(self.heap.len())
            .saturating_sub(index)
    }

    pub fn row_count(&self) -> usize {
        self.row_offsets.len()
    }

    /// Renders the page. Every row is marked present.
    pub fn finish(self) -> Vec<u8> {
        let mut page = vec![0_u8; self.page_size];
        let num_rows = self.row_offsets.len();

        // Header.
        page[0x04..0x08].copy_from_slice(&self.page_index.to_le_bytes());
        page[0x08..0x0c].copy_from_slice(&self.page_type.to_le_bytes());
        page[0x0c..0x10].copy_from_slice(&self.next_page.to_le_bytes());
        page[0x18] = u8::try_from(num_rows.min(0xff)).unwrap_or(0xff);
        page[0x1b] = 0x24; // ordinary data page
        let used = u16::try_from(self.heap.len()).unwrap_or(u16::MAX);
        page[0x1c..0x1e].copy_from_slice(&u16::try_from(self.free_space()).unwrap_or(0).to_le_bytes());
        page[0x1e..0x20].copy_from_slice(&used.to_le_bytes());
        if num_rows > 0xff {
            page[0x22..0x24]
                .copy_from_slice(&u16::try_from(num_rows).unwrap_or(u16::MAX).to_le_bytes());
        }

        // Heap.
        let end = (PAGE_HEADER_LEN + self.heap.len()).min(page.len());
        page[PAGE_HEADER_LEN..end].copy_from_slice(&self.heap[..end - PAGE_HEADER_LEN]);

        // Row index, backwards from the end, sixteen rows per group.
        let groups = num_rows.div_ceil(16);
        for group in 0..groups {
            let base = self.page_size - group * ROW_GROUP_LEN;
            let in_group = if group < groups - 1 { 16 } else { num_rows - group * 16 };
            let mut present: u16 = 0;
            for row in 0..in_group {
                present |= 1 << row;
                let offset = self.row_offsets.get(group * 16 + row).copied().unwrap_or(0);
                let at = base - (6 + 2 * row);
                if at + 2 <= page.len() {
                    page[at..at + 2].copy_from_slice(&offset.to_le_bytes());
                }
            }
            let flags_at = base - 4;
            if flags_at + 2 <= page.len() {
                page[flags_at..flags_at + 2].copy_from_slice(&present.to_le_bytes());
            }
        }

        page
    }
}

/// Builds a whole file: header page listing the tables, then their pages.
pub struct FileBuilder {
    page_size: usize,
    /// (`page_type`, pages)
    tables: Vec<(u32, Vec<Vec<u8>>)>,
}

impl FileBuilder {
    pub fn new(page_size: usize) -> Self {
        Self { page_size, tables: Vec::new() }
    }

    /// Adds a table whose rows are already encoded.
    ///
    /// The first page of a table is a placeholder that carries no rows, which is
    /// what rekordbox's own files do.
    pub fn add_table(&mut self, page_type: u32, rows: &[Vec<u8>]) {
        // Page indices are assigned in `finish`, so use a placeholder for now
        // and patch the links afterwards.
        let mut pages: Vec<Vec<u8>> = Vec::new();
        let mut builder = PageBuilder::new(self.page_size, 0, page_type, 0);
        for row in rows {
            if builder.free_space() < row.len() + 8 {
                pages.push(std::mem::replace(
                    &mut builder,
                    PageBuilder::new(self.page_size, 0, page_type, 0),
                )
                .finish());
            }
            builder.push_row(row);
        }
        pages.push(builder.finish());
        self.tables.push((page_type, pages));
    }

    pub fn finish(self) -> Vec<u8> {
        let header_pages = 1;
        // Each table gets an empty first page plus its data pages.
        let mut out = vec![0_u8; self.page_size * header_pages];

        let num_tables = self.tables.len();
        out[0x04..0x08].copy_from_slice(&u32::try_from(self.page_size).unwrap_or(4096).to_le_bytes());
        out[0x08..0x0c].copy_from_slice(&u32::try_from(num_tables).unwrap_or(0).to_le_bytes());
        out[0x14..0x18].copy_from_slice(&1_u32.to_le_bytes()); // sequence

        let mut next_index = u32::try_from(header_pages).unwrap_or(1);
        let mut entries = Vec::new();

        for (page_type, pages) in self.tables {
            let first = next_index;
            let count = u32::try_from(pages.len()).unwrap_or(1);
            let last = first + count - 1;
            for (i, mut page) in pages.into_iter().enumerate() {
                let index = first + u32::try_from(i).unwrap_or(0);
                page[0x04..0x08].copy_from_slice(&index.to_le_bytes());
                let next = if index == last { last } else { index + 1 };
                page[0x0c..0x10].copy_from_slice(&next.to_le_bytes());
                out.extend_from_slice(&page);
            }
            entries.push((page_type, first, last));
            next_index = last + 1;
        }

        for (i, (page_type, first, last)) in entries.into_iter().enumerate() {
            let at = 28 + i * 16;
            out[at..at + 4].copy_from_slice(&page_type.to_le_bytes());
            out[at + 8..at + 12].copy_from_slice(&first.to_le_bytes());
            out[at + 12..at + 16].copy_from_slice(&last.to_le_bytes());
        }

        out[0x0c..0x10].copy_from_slice(&next_index.to_le_bytes()); // next unused page
        out
    }
}
