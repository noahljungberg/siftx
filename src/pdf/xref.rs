//! PDF cross-reference table parser (XR1-XR10).
//!
//! Parses xref tables (traditional and stream-based), trailer dictionaries,
//! incremental updates, and resolves indirect references.
//! Per ISO 32000-2 §7.5.

use super::decode;
use super::encrypt::SecurityHandler;
use super::object::{Parser, PdfObject, Ref};
use super::tokenizer::{Keyword, Token, Tokenizer};
use crate::core::{Error, Result};

/// A single cross-reference entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XRefEntry {
    /// Free entry (deleted object).
    Free { next_free: u32, generation: u16 },
    /// Uncompressed object at a byte offset in the file.
    Uncompressed { offset: u64, generation: u16 },
    /// Object compressed inside an object stream (PDF 1.5+).
    Compressed {
        /// Object number of the object stream containing this object.
        stream_obj: u32,
        /// Index of this object within the object stream.
        index: u32,
    },
}

/// The merged cross-reference table for a PDF document.
#[derive(Debug)]
pub struct XRefTable {
    /// Entries indexed by object number. `None` means unused slot.
    pub entries: Vec<Option<XRefEntry>>,
    /// The merged trailer dictionary (latest takes precedence).
    pub trailer: PdfObject,
}

impl XRefTable {
    /// Look up an entry by object number.
    pub fn get(&self, obj_num: u32) -> Option<&XRefEntry> {
        self.entries.get(obj_num as usize)?.as_ref()
    }

    /// Number of entries (including gaps).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Insert an entry, growing the table if needed.
    /// Only inserts if the slot is currently empty (first xref section wins
    /// for each object in the /Prev chain, since we process newest first).
    fn insert_if_absent(&mut self, obj_num: u32, entry: XRefEntry) {
        let idx = obj_num as usize;
        if idx >= self.entries.len() {
            self.entries.resize(idx + 1, None);
        }
        if self.entries[idx].is_none() {
            self.entries[idx] = Some(entry);
        }
    }
}

/// XR9: Resolve an indirect reference to a parsed object.
///
/// Looks up the reference in the xref table, seeks to the offset,
/// parses the indirect object definition, and returns the inner object.
pub fn resolve_reference(
    data: &[u8],
    xref: &XRefTable,
    reference: Ref,
    security: Option<&SecurityHandler>,
) -> Result<PdfObject> {
    let entry = xref.get(reference.num).ok_or_else(|| {
        Error::Format(format!(
            "object {} {} R not in xref",
            reference.num, reference.generation
        ))
    })?;

    match entry {
        XRefEntry::Uncompressed { offset, .. } => {
            let mut parser = Parser::new(data);
            parser.seek(*offset as usize);
            match parser.parse_indirect_object() {
                Ok((num, _gen, obj)) if num == reference.num => Ok(obj),
                Ok(_) | Err(_) => {
                    // Object number mismatch (corrupt xref) or parse error.
                    // Search nearby first, then fall back to full file scan.
                    resolve_nearby(data, *offset as usize, reference)
                        .or_else(|_| resolve_by_scan(data, reference))
                }
            }
        }
        XRefEntry::Compressed { stream_obj, index } => {
            // XR10: Need to decompress the object stream and extract the object
            resolve_from_object_stream(data, xref, *stream_obj, *index, security)
        }
        XRefEntry::Free { .. } => {
            // Normally free objects resolve to Null. But some malformed PDFs
            // have corrupt xref tables that incorrectly mark objects as free.
            // If something references a free object, try scanning the file.
            resolve_by_scan(data, reference).or(Ok(PdfObject::Null))
        }
    }
}

/// Try to find and parse an object near the given offset.
/// Searches ±10 bytes for a valid "N G obj" pattern matching the expected object number.
fn resolve_nearby(data: &[u8], offset: usize, reference: Ref) -> Result<PdfObject> {
    let search_start = offset.saturating_sub(10);
    let search_end = (offset + 10).min(data.len());

    // Build the expected prefix: "N G obj"
    let prefix = format!("{} {} obj", reference.num, reference.generation);
    let prefix_bytes = prefix.as_bytes();

    for pos in search_start..search_end {
        if pos + prefix_bytes.len() > data.len() {
            break;
        }
        if &data[pos..pos + prefix_bytes.len()] == prefix_bytes {
            // Verify the byte after "obj" is whitespace or delimiter
            let after = pos + prefix_bytes.len();
            if after < data.len()
                && !matches!(data[after], 0 | 9 | 10 | 12 | 13 | 32 | b'<' | b'[' | b'(')
            {
                continue;
            }
            let mut parser = Parser::new(data);
            parser.seek(pos);
            if let Ok((_num, _gen, obj)) = parser.parse_indirect_object() {
                return Ok(obj);
            }
        }
    }

    Err(Error::Format(format!(
        "object {} {} R not found near offset {}",
        reference.num, reference.generation, offset
    )))
}

/// Scan the entire file for `N G obj` matching the expected reference.
/// Used as last resort when the xref entry points to the wrong object.
fn resolve_by_scan(data: &[u8], reference: Ref) -> Result<PdfObject> {
    let prefix = format!("{} {} obj", reference.num, reference.generation);
    let prefix_bytes = prefix.as_bytes();

    for pos in 0..data.len().saturating_sub(prefix_bytes.len()) {
        // Must be at line start or file start
        if pos > 0 && data[pos - 1] != b'\n' && data[pos - 1] != b'\r' {
            continue;
        }
        if &data[pos..pos + prefix_bytes.len()] != prefix_bytes {
            continue;
        }
        let after = pos + prefix_bytes.len();
        if after < data.len()
            && !matches!(data[after], 0 | 9 | 10 | 12 | 13 | 32 | b'<' | b'[' | b'(')
        {
            continue;
        }
        let mut parser = Parser::new(data);
        parser.seek(pos);
        if let Ok((num, _gen, obj)) = parser.parse_indirect_object() {
            if num == reference.num {
                return Ok(obj);
            }
        }
    }

    Err(Error::Format(format!(
        "object {} {} R not found by scan",
        reference.num, reference.generation
    )))
}

/// Recursively resolve: if the object is a Ref, look it up.
pub fn resolve_deep(data: &[u8], xref: &XRefTable, obj: &PdfObject) -> Result<PdfObject> {
    match obj {
        PdfObject::Ref(r) => resolve_reference(data, xref, *r, None),
        other => Ok(other.clone()),
    }
}

// --- XR1: Find startxref ---

/// XR1: Find the `startxref` offset by scanning backwards from EOF.
///
/// Returns the byte offset value that follows the `startxref` keyword.
pub fn find_startxref(data: &[u8]) -> Result<u64> {
    // Scan the last 1024 bytes (spec says search last 1KB)
    let search_start = data.len().saturating_sub(1024);
    let tail = &data[search_start..];

    // Find last occurrence of "startxref"
    let needle = b"startxref";
    let mut found = None;
    for i in 0..tail.len().saturating_sub(needle.len()) {
        if &tail[i..i + needle.len()] == needle {
            found = Some(search_start + i);
        }
    }

    let pos = found.ok_or_else(|| Error::Format("startxref not found".into()))?;

    // Parse the offset number after "startxref"
    let after = pos + needle.len();
    let mut tok = Tokenizer::new(data);
    tok.seek(after);
    tok.skip_whitespace();

    match tok.next_token()? {
        Some(Token::Int(offset)) if offset >= 0 => Ok(offset as u64),
        _ => Err(Error::Format("invalid startxref offset".into())),
    }
}

// --- XR2 + XR3: Parse traditional xref table + trailer ---

/// Parse a complete xref table section at the given offset, returning
/// entries and trailer dictionary.
fn parse_xref_table_section(
    data: &[u8],
    offset: u64,
) -> Result<(Vec<(u32, XRefEntry)>, PdfObject)> {
    let mut tok = Tokenizer::new(data);
    tok.seek(offset as usize);
    tok.skip_whitespace();

    // Expect "xref" keyword
    match tok.next_token()? {
        Some(Token::Keyword(Keyword::Xref)) => {}
        _ => return Err(Error::Format("expected 'xref' keyword".into())),
    }

    let mut entries = Vec::new();

    // XR2: Parse subsections
    loop {
        tok.skip_whitespace();
        let saved = tok.position();

        // Check if next token is "trailer" (end of xref table)
        match tok.next_token()? {
            Some(Token::Keyword(Keyword::Trailer)) => break,
            Some(Token::Int(start_obj)) => {
                // Subsection: start_obj count
                let count = match tok.next_token()? {
                    Some(Token::Int(c)) if c >= 0 => c as u32,
                    _ => return Err(Error::Format("expected xref subsection count".into())),
                };

                // Parse entries: "offset generation f|n"
                for i in 0..count {
                    tok.skip_whitespace();
                    let entry_offset = match tok.next_token()? {
                        Some(Token::Int(o)) if o >= 0 => o as u64,
                        _ => return Err(Error::Format("expected xref entry offset".into())),
                    };
                    let generation = match tok.next_token()? {
                        Some(Token::Int(g)) if g >= 0 && g <= u16::MAX as i64 => g as u16,
                        _ => return Err(Error::Format("expected xref entry generation".into())),
                    };

                    // Read "f" or "n" - these aren't keywords in our tokenizer,
                    // so we need to read them manually
                    tok.skip_whitespace();
                    let type_pos = tok.position();
                    if type_pos >= data.len() {
                        return Err(Error::Format("truncated xref entry".into()));
                    }
                    let type_byte = data[type_pos];
                    tok.seek(type_pos + 1);

                    let obj_num = start_obj as u32 + i;
                    let entry = match type_byte {
                        b'f' => XRefEntry::Free {
                            next_free: entry_offset as u32,
                            generation,
                        },
                        b'n' => XRefEntry::Uncompressed {
                            offset: entry_offset,
                            generation,
                        },
                        _ => {
                            return Err(Error::Format(format!(
                                "expected 'f' or 'n' in xref entry, got 0x{:02X}",
                                type_byte
                            )));
                        }
                    };
                    entries.push((obj_num, entry));
                }
            }
            _ => {
                tok.seek(saved);
                return Err(Error::Format(
                    "expected subsection start or 'trailer'".into(),
                ));
            }
        }
    }

    // XR3: Parse trailer dictionary
    let mut parser = Parser::new(data);
    parser.seek(tok.position());
    let trailer = parser.parse_object()?;

    Ok((entries, trailer))
}

// --- XR5 + XR6: Parse cross-reference stream ---

/// Parse a cross-reference stream at the given offset.
fn parse_xref_stream(data: &[u8], offset: u64) -> Result<(Vec<(u32, XRefEntry)>, PdfObject)> {
    let mut parser = Parser::new(data);
    parser.seek(offset as usize);
    let (_num, _gen, obj) = parser.parse_indirect_object()?;

    let dict = obj
        .as_dict()
        .ok_or_else(|| Error::Format("xref stream is not a stream".into()))?;

    // Verify /Type /XRef
    if let Some(type_obj) = obj.dict_get(b"Type") {
        if type_obj.as_name() != Some(b"XRef") {
            return Err(Error::Format("xref stream /Type is not /XRef".into()));
        }
    }

    // Get /W array (field widths)
    let w_array = obj
        .dict_get(b"W")
        .and_then(|w| w.as_array())
        .ok_or_else(|| Error::Format("xref stream missing /W array".into()))?;

    if w_array.len() != 3 {
        return Err(Error::Format("xref stream /W must have 3 elements".into()));
    }

    let w: [usize; 3] = [
        w_array[0].as_int().unwrap_or(0) as usize,
        w_array[1].as_int().unwrap_or(0) as usize,
        w_array[2].as_int().unwrap_or(0) as usize,
    ];
    let entry_size = w[0] + w[1] + w[2];

    // Get /Size
    let size = obj
        .dict_get(b"Size")
        .and_then(|s| s.as_int())
        .ok_or_else(|| Error::Format("xref stream missing /Size".into()))? as u32;

    // Get /Index array (default: [0 Size])
    let index_pairs: Vec<(u32, u32)> = if let Some(idx) = obj.dict_get(b"Index") {
        let arr = idx
            .as_array()
            .ok_or_else(|| Error::Format("xref stream /Index is not an array".into()))?;
        if arr.len() % 2 != 0 {
            return Err(Error::Format(
                "xref stream /Index must have even length".into(),
            ));
        }
        arr.chunks(2)
            .map(|pair| {
                let start = pair[0].as_int().unwrap_or(0) as u32;
                let count = pair[1].as_int().unwrap_or(0) as u32;
                (start, count)
            })
            .collect()
    } else {
        vec![(0, size)]
    };

    // Decompress stream data
    let raw_stream = obj
        .stream_data()
        .ok_or_else(|| Error::Format("xref stream has no stream data".into()))?;

    let stream_data = decompress_xref_stream(&obj, raw_stream)?;

    // XR6: Decode entries
    let mut entries = Vec::new();
    let mut data_pos = 0;

    for (start, count) in &index_pairs {
        for i in 0..*count {
            if data_pos + entry_size > stream_data.len() {
                break;
            }

            let field0 = read_field(&stream_data[data_pos..], w[0]);
            let field1 = read_field(&stream_data[data_pos + w[0]..], w[1]);
            let field2 = read_field(&stream_data[data_pos + w[0] + w[1]..], w[2]);
            data_pos += entry_size;

            // Default type is 1 if w[0] == 0
            let entry_type = if w[0] == 0 { 1 } else { field0 };

            let obj_num = start + i;
            let entry = match entry_type {
                0 => XRefEntry::Free {
                    next_free: field1 as u32,
                    generation: field2 as u16,
                },
                1 => XRefEntry::Uncompressed {
                    offset: field1,
                    generation: field2 as u16,
                },
                2 => XRefEntry::Compressed {
                    stream_obj: field1 as u32,
                    index: field2 as u32,
                },
                _ => continue, // Unknown type - skip
            };
            entries.push((obj_num, entry));
        }
    }

    // The stream object itself serves as the trailer dict
    let trailer = PdfObject::Dict(dict.to_vec());

    Ok((entries, trailer))
}

/// Decompress xref stream data based on /Filter.
fn decompress_xref_stream(obj: &PdfObject, raw: &[u8]) -> Result<Vec<u8>> {
    let filter = obj.dict_get(b"Filter");

    let decompressed = match filter {
        Some(PdfObject::Name(name)) if name == b"FlateDecode" => decode::flate_decompress(raw)?,
        None => raw.to_vec(),
        Some(other) => {
            return Err(Error::Unsupported(format!(
                "xref stream filter: {:?}",
                other
            )));
        }
    };

    // Check for /DecodeParms with /Predictor
    if let Some(parms) = obj.dict_get(b"DecodeParms") {
        let predictor = parms
            .dict_get(b"Predictor")
            .and_then(|p| p.as_int())
            .unwrap_or(1);

        if predictor >= 10 {
            // PNG predictor
            let columns = parms
                .dict_get(b"Columns")
                .and_then(|c| c.as_int())
                .unwrap_or(1) as usize;
            return decode::apply_png_predictor(&decompressed, columns);
        }
    }

    Ok(decompressed)
}

/// Read a variable-width big-endian integer field from a byte slice.
fn read_field(data: &[u8], width: usize) -> u64 {
    let mut val: u64 = 0;
    for i in 0..width.min(data.len()) {
        val = (val << 8) | data[i] as u64;
    }
    val
}

// --- XR4: Follow /Prev chain ---

/// Build a complete XRefTable by following the /Prev chain.
///
/// This is the main entry point for xref parsing. It finds startxref,
/// parses each xref section, and merges them (newest first, so first
/// occurrence wins for each object number).
pub fn build_xref_table(data: &[u8]) -> Result<XRefTable> {
    build_xref_table_inner(data, false)
}

/// Build xref table with nearby-xref fallback for off-by-N startxref/Prev offsets.
/// Used when the standard `build_xref_table` fails.
pub fn build_xref_table_nearby(data: &[u8]) -> Result<XRefTable> {
    build_xref_table_inner(data, true)
}

fn build_xref_table_inner(data: &[u8], use_nearby_fallback: bool) -> Result<XRefTable> {
    let startxref = find_startxref(data)?;

    let mut table = XRefTable {
        entries: Vec::new(),
        trailer: PdfObject::Null,
    };

    let mut offset = Some(startxref);
    let mut first_trailer = true;
    // Track visited offsets to prevent infinite loops
    let mut visited = std::collections::HashSet::new();

    while let Some(xref_offset) = offset {
        if !visited.insert(xref_offset) {
            break; // Cycle detected
        }

        // Try xref stream first, then traditional table
        let result = if use_nearby_fallback {
            parse_xref_at(data, xref_offset)
                .or_else(|_| try_nearby_xref_table(data, xref_offset as usize))
        } else {
            parse_xref_at(data, xref_offset)
        };

        let (entries, trailer) = result?;

        for (obj_num, entry) in entries {
            table.insert_if_absent(obj_num, entry);
        }

        if first_trailer {
            table.trailer = trailer.clone();
            first_trailer = false;
        }

        // XR4: Follow /Prev
        offset = trailer
            .dict_get(b"Prev")
            .and_then(|p| p.as_int())
            .filter(|&p| p >= 0)
            .map(|p| p as u64);
    }

    if table.trailer.is_null() {
        return Err(Error::Format("no trailer dictionary found".into()));
    }

    Ok(table)
}

/// Parse xref at a given offset - auto-detects traditional table vs xref stream.
fn parse_xref_at(data: &[u8], offset: u64) -> Result<(Vec<(u32, XRefEntry)>, PdfObject)> {
    let pos = offset as usize;
    if pos >= data.len() {
        return Err(Error::Format(format!("xref offset {} beyond EOF", offset)));
    }

    // XR7: Check if it's a traditional "xref" table or an xref stream
    // Traditional starts with "xref", stream starts with "N G obj"
    let mut tok = Tokenizer::new(data);
    tok.seek(pos);
    tok.skip_whitespace();

    let saved = tok.position();
    match tok.next_token()? {
        Some(Token::Keyword(Keyword::Xref)) => {
            // Traditional xref table
            parse_xref_table_section(data, offset)
        }
        Some(Token::Int(_)) => {
            // Could be xref stream: "N G obj << /Type /XRef ... >> stream ..."
            tok.seek(saved);
            parse_xref_stream(data, offset)
        }
        _ => Err(Error::Format(format!(
            "expected 'xref' or xref stream at offset {}",
            offset
        ))),
    }
}

/// Search backwards up to 10 bytes from `pos` for a `xref` keyword and parse
/// the traditional xref table starting there. This handles malformed PDFs where
/// `startxref` or `/Prev` offsets point a few bytes past the `xref` keyword.
fn try_nearby_xref_table(data: &[u8], pos: usize) -> Result<(Vec<(u32, XRefEntry)>, PdfObject)> {
    let search_start = pos.saturating_sub(10);
    // Extend past pos to capture "xref" keywords that start before pos
    let search_end = (pos + 4).min(data.len());
    if search_start >= search_end || search_end - search_start < 4 {
        return Err(Error::Format("no nearby xref found".into()));
    }
    let window = &data[search_start..search_end];
    // Search backwards for "xref" - rposition gives the last (closest) match.
    // Exclude matches that are part of "startxref".
    for idx in (0..window.len().saturating_sub(3)).rev() {
        if &window[idx..idx + 4] == b"xref" {
            let abs_pos = search_start + idx;
            // Make sure this isn't part of "startxref"
            if abs_pos >= 5 && &data[abs_pos - 5..abs_pos] == b"start" {
                continue;
            }
            return parse_xref_table_section(data, abs_pos as u64);
        }
    }
    Err(Error::Format(format!(
        "no nearby xref found at offset {}",
        pos
    )))
}

// --- XR8: XRef reconstruction ---

/// XR8: Reconstruct xref by scanning file for `N G obj` patterns.
///
/// Used as fallback when the xref table is corrupt or missing.
pub fn reconstruct_xref(data: &[u8]) -> Result<XRefTable> {
    let mut entries = Vec::new();
    let mut pos = 0;

    while pos < data.len() {
        // Look for pattern: digits + space + digits + space + "obj"
        if let Some((obj_num, generation, obj_offset)) = try_match_obj_header(data, pos) {
            entries.push((
                obj_num,
                XRefEntry::Uncompressed {
                    offset: obj_offset as u64,
                    generation,
                },
            ));
            pos = obj_offset + 1;
        } else {
            pos += 1;
        }
    }

    // Try to find a trailer by scanning backwards for "trailer"
    let trailer = find_last_trailer(data).unwrap_or(PdfObject::Null);

    let mut table = XRefTable {
        entries: Vec::new(),
        trailer,
    };

    for (obj_num, entry) in entries {
        table.insert_if_absent(obj_num, entry);
    }

    Ok(table)
}

/// Try to match "N G obj" at the given position.
/// Returns (object_number, generation, start_offset) if matched.
fn try_match_obj_header(data: &[u8], start: usize) -> Option<(u32, u16, usize)> {
    // Must be at start of line or start of file
    if start > 0 && data[start - 1] != b'\n' && data[start - 1] != b'\r' {
        return None;
    }

    let mut pos = start;

    // Parse object number (digits)
    let num_start = pos;
    while pos < data.len() && data[pos].is_ascii_digit() {
        pos += 1;
    }
    if pos == num_start || pos >= data.len() {
        return None;
    }
    let num: u32 = std::str::from_utf8(&data[num_start..pos])
        .ok()?
        .parse()
        .ok()?;

    // Space
    if pos >= data.len() || data[pos] != b' ' {
        return None;
    }
    pos += 1;

    // Parse generation (digits)
    let gen_start = pos;
    while pos < data.len() && data[pos].is_ascii_digit() {
        pos += 1;
    }
    if pos == gen_start || pos >= data.len() {
        return None;
    }
    let generation: u16 = std::str::from_utf8(&data[gen_start..pos])
        .ok()?
        .parse()
        .ok()?;

    // Space
    if pos >= data.len() || data[pos] != b' ' {
        return None;
    }
    pos += 1;

    // "obj" keyword followed by whitespace or delimiter
    if pos + 3 > data.len() || &data[pos..pos + 3] != b"obj" {
        return None;
    }
    pos += 3;
    if pos < data.len() && !matches!(data[pos], 0 | 9 | 10 | 12 | 13 | 32 | b'<' | b'[' | b'(') {
        return None;
    }

    Some((num, generation, start))
}

/// Find the last "trailer << ... >>" in the file.
fn find_last_trailer(data: &[u8]) -> Option<PdfObject> {
    let needle = b"trailer";
    let mut last_pos = None;

    for i in 0..data.len().saturating_sub(needle.len()) {
        if &data[i..i + needle.len()] == needle {
            last_pos = Some(i);
        }
    }

    let pos = last_pos?;
    let mut parser = Parser::new(data);
    parser.seek(pos + needle.len());
    parser.parse_object().ok()
}

// --- XR10: Object streams ---

/// XR10: Extract an object from an object stream.
fn resolve_from_object_stream(
    data: &[u8],
    xref: &XRefTable,
    stream_obj_num: u32,
    index: u32,
    security: Option<&SecurityHandler>,
) -> Result<PdfObject> {
    // First resolve the object stream itself
    let stream_entry = xref
        .get(stream_obj_num)
        .ok_or_else(|| Error::Format(format!("object stream {} not in xref", stream_obj_num)))?;

    let stream_offset = match stream_entry {
        XRefEntry::Uncompressed { offset, .. } => *offset,
        _ => {
            return Err(Error::Format(format!(
                "object stream {} is not uncompressed",
                stream_obj_num
            )));
        }
    };

    let mut parser = Parser::new(data);
    parser.seek(stream_offset as usize);
    let (_num, _gen, stream_obj) = parser.parse_indirect_object()?;

    // Verify /Type /ObjStm
    if let Some(type_obj) = stream_obj.dict_get(b"Type") {
        if type_obj.as_name() != Some(b"ObjStm") {
            return Err(Error::Format("expected /Type /ObjStm".into()));
        }
    }

    let n = stream_obj
        .dict_get(b"N")
        .and_then(|n| n.as_int())
        .ok_or_else(|| Error::Format("object stream missing /N".into()))? as u32;

    let first = stream_obj
        .dict_get(b"First")
        .and_then(|f| f.as_int())
        .ok_or_else(|| Error::Format("object stream missing /First".into()))?
        as usize;

    // Decrypt stream data if encrypted, then decompress.
    // Object streams use the stream object's own number (gen 0 per spec §7.5.7).
    let raw_stream = stream_obj
        .stream_data()
        .ok_or_else(|| Error::Format("object stream has no data".into()))?;

    let decrypted;
    let raw_for_decompress = if let Some(handler) = security.filter(|h| h.is_authenticated()) {
        decrypted = handler
            .decrypt_stream(stream_obj_num, 0, raw_stream)
            .unwrap_or_else(|_| raw_stream.to_vec());
        &decrypted
    } else {
        raw_stream
    };

    let stream_data = decompress_objstm(&stream_obj, raw_for_decompress)?;

    if index >= n {
        return Err(Error::Format(format!(
            "object stream index {} >= N {}",
            index, n
        )));
    }

    // Parse the N pairs of (obj_num, offset) from the header
    let header = &stream_data[..first.min(stream_data.len())];
    let mut header_tok = Tokenizer::new(header);
    let mut offsets = Vec::with_capacity(n as usize);

    for _ in 0..n {
        let _obj_num = match header_tok.next_token()? {
            Some(Token::Int(v)) => v,
            _ => return Err(Error::Format("invalid object stream header".into())),
        };
        let obj_offset = match header_tok.next_token()? {
            Some(Token::Int(v)) if v >= 0 => v as usize,
            _ => return Err(Error::Format("invalid object stream header".into())),
        };
        offsets.push(obj_offset);
    }

    // Parse the object at the given index
    let obj_start = first + offsets[index as usize];
    if obj_start >= stream_data.len() {
        return Err(Error::Format("object stream offset out of range".into()));
    }

    let mut obj_parser = Parser::new(&stream_data[obj_start..]);
    obj_parser.parse_object()
}

/// Decompress object stream data.
fn decompress_objstm(obj: &PdfObject, raw: &[u8]) -> Result<Vec<u8>> {
    let filter = obj.dict_get(b"Filter");

    match filter {
        Some(PdfObject::Name(name)) if name == b"FlateDecode" => decode::flate_decompress(raw),
        // Handle filter as single-element array: [/FlateDecode]
        Some(PdfObject::Array(arr)) if arr.len() == 1 => {
            if arr[0].as_name() == Some(b"FlateDecode") {
                decode::flate_decompress(raw)
            } else {
                Err(Error::Unsupported(format!(
                    "object stream filter: {:?}",
                    arr[0]
                )))
            }
        }
        None => Ok(raw.to_vec()),
        Some(other) => Err(Error::Unsupported(format!(
            "object stream filter: {:?}",
            other
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Helper to build a minimal PDF ---

    fn build_minimal_pdf(objects: &[(u32, &[u8])], trailer_extra: &str) -> Vec<u8> {
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let mut xref_entries: Vec<(u32, usize)> = Vec::new();

        // Write objects
        for &(num, body) in objects {
            xref_entries.push((num, pdf.len()));
            pdf.extend_from_slice(format!("{} 0 obj\n", num).as_bytes());
            pdf.extend_from_slice(body);
            pdf.extend_from_slice(b"\nendobj\n");
        }

        // Write xref table
        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n");

        // Find max obj number
        let max_obj = xref_entries.iter().map(|(n, _)| *n).max().unwrap_or(0);
        pdf.extend_from_slice(format!("0 {}\n", max_obj + 1).as_bytes());

        // Object 0 is always free
        pdf.extend_from_slice(b"0000000000 65535 f \n");

        for obj_num in 1..=max_obj {
            if let Some((_, offset)) = xref_entries.iter().find(|(n, _)| *n == obj_num) {
                pdf.extend_from_slice(format!("{:010} {:05} n \n", offset, 0).as_bytes());
            } else {
                pdf.extend_from_slice(b"0000000000 00000 f \n");
            }
        }

        // Trailer
        let size = max_obj + 1;
        pdf.extend_from_slice(
            format!("trailer\n<< /Size {} {} >>\n", size, trailer_extra).as_bytes(),
        );
        pdf.extend_from_slice(format!("startxref\n{}\n%%EOF", xref_offset).as_bytes());

        pdf
    }

    // --- XR1: Find startxref ---

    #[test]
    fn xr1_find_startxref() {
        let pdf = build_minimal_pdf(&[(1, b"null")], "");
        let offset = find_startxref(&pdf).unwrap();
        assert!(offset > 0);
    }

    #[test]
    fn xr1_no_startxref() {
        assert!(find_startxref(b"%PDF-1.7\nno xref here\n%%EOF").is_err());
    }

    // --- XR2: Parse xref table ---

    #[test]
    fn xr2_parse_xref_table() {
        let pdf = build_minimal_pdf(&[(1, b"(Hello)")], "");
        let startxref = find_startxref(&pdf).unwrap();
        let (entries, _trailer) = parse_xref_table_section(&pdf, startxref).unwrap();

        // Should have object 0 (free) and object 1 (in-use)
        assert!(entries.len() >= 2);

        let free_entry = entries.iter().find(|(n, _)| *n == 0).unwrap();
        assert!(matches!(free_entry.1, XRefEntry::Free { .. }));

        let obj1_entry = entries.iter().find(|(n, _)| *n == 1).unwrap();
        assert!(matches!(obj1_entry.1, XRefEntry::Uncompressed { .. }));
    }

    // --- XR3: Trailer dictionary ---

    #[test]
    fn xr3_trailer_dict() {
        let pdf = build_minimal_pdf(&[(1, b"null")], "/Root 1 0 R");
        let table = build_xref_table(&pdf).unwrap();

        assert_eq!(
            table.trailer.dict_get(b"Root"),
            Some(&PdfObject::Ref(Ref {
                num: 1,
                generation: 0
            }))
        );
    }

    // --- XR4: /Prev chain ---

    #[test]
    fn xr4_prev_chain() {
        // Build a PDF with two xref sections simulating an incremental update
        let mut pdf = b"%PDF-1.7\n".to_vec();

        // Object 1
        let obj1_offset = pdf.len();
        pdf.extend_from_slice(b"1 0 obj\n(Original)\nendobj\n");

        // First xref
        let xref1_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 2\n");
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj1_offset, 0).as_bytes());
        pdf.extend_from_slice(b"trailer\n<< /Size 2 >>\n");
        pdf.extend_from_slice(format!("startxref\n{}\n", xref1_offset).as_bytes());

        // Object 2 (added in update)
        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n(Added)\nendobj\n");

        // Second xref (references first via /Prev)
        let xref2_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n2 1\n");
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj2_offset, 0).as_bytes());
        pdf.extend_from_slice(
            format!("trailer\n<< /Size 3 /Prev {} >>\n", xref1_offset).as_bytes(),
        );
        pdf.extend_from_slice(format!("startxref\n{}\n%%EOF", xref2_offset).as_bytes());

        let table = build_xref_table(&pdf).unwrap();

        // Should have entries for objects 0, 1, and 2
        assert!(table.get(0).is_some()); // free
        assert!(table.get(1).is_some()); // from first xref
        assert!(table.get(2).is_some()); // from second xref

        // Verify trailer has /Size from the newest section
        assert_eq!(table.trailer.dict_get(b"Size"), Some(&PdfObject::Int(3)));
    }

    // --- XR5 + XR6: Xref streams ---

    #[test]
    fn xr5_xr6_xref_stream() {
        // Build a PDF with an xref stream (uncompressed for testing)
        let mut pdf = b"%PDF-1.7\n".to_vec();

        // Object 1 (a simple dict)
        let obj1_offset = pdf.len();
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");

        // Object 2: xref stream
        let xref_obj_offset = pdf.len();

        // XRef stream entries: W = [1, 2, 1]
        // Entry for obj 0: type=0 (free), next_free=0, gen=255
        // Entry for obj 1: type=1, offset=obj1_offset, gen=0
        // Entry for obj 2: type=1, offset=xref_obj_offset, gen=0
        let mut stream_data = Vec::new();
        // obj 0: free
        stream_data.push(0); // type 0
        stream_data.extend_from_slice(&(0u16).to_be_bytes()); // next_free
        stream_data.push(255); // gen
        // obj 1: uncompressed
        stream_data.push(1); // type 1
        stream_data.extend_from_slice(&(obj1_offset as u16).to_be_bytes()); // offset
        stream_data.push(0); // gen
        // obj 2: uncompressed (self)
        stream_data.push(1); // type 1
        stream_data.extend_from_slice(&(xref_obj_offset as u16).to_be_bytes()); // offset
        stream_data.push(0); // gen

        let stream_len = stream_data.len();
        pdf.extend_from_slice(
            format!(
                "2 0 obj\n<< /Type /XRef /Size 3 /W [1 2 1] /Length {} /Root 1 0 R >>\nstream\n",
                stream_len
            )
            .as_bytes(),
        );
        pdf.extend_from_slice(&stream_data);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");

        pdf.extend_from_slice(format!("startxref\n{}\n%%EOF", xref_obj_offset).as_bytes());

        let table = build_xref_table(&pdf).unwrap();

        assert!(matches!(table.get(0), Some(XRefEntry::Free { .. })));
        assert!(matches!(table.get(1), Some(XRefEntry::Uncompressed { .. })));
        assert!(matches!(table.get(2), Some(XRefEntry::Uncompressed { .. })));

        // Trailer comes from the xref stream dict
        assert_eq!(
            table.trailer.dict_get(b"Root"),
            Some(&PdfObject::Ref(Ref {
                num: 1,
                generation: 0
            }))
        );
    }

    // --- XR8: XRef reconstruction ---

    #[test]
    fn xr8_reconstruct() {
        let mut pdf = b"%PDF-1.7\n".to_vec();

        let obj1_offset = pdf.len();
        pdf.extend_from_slice(b"1 0 obj\n(Hello)\nendobj\n");

        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n42\nendobj\n");

        // No xref or trailer - corrupt file
        pdf.extend_from_slice(b"%%EOF");

        let table = reconstruct_xref(&pdf).unwrap();

        // Should have found both objects
        assert!(matches!(
            table.get(1),
            Some(XRefEntry::Uncompressed { offset, .. }) if *offset == obj1_offset as u64
        ));
        assert!(matches!(
            table.get(2),
            Some(XRefEntry::Uncompressed { offset, .. }) if *offset == obj2_offset as u64
        ));
    }

    #[test]
    fn xr8_reconstruct_skips_non_objects() {
        let pdf = b"%PDF-1.7\nsome random text 123\n%%EOF";
        let table = reconstruct_xref(pdf).unwrap();
        assert!(table.is_empty() || table.entries.iter().all(|e| e.is_none()));
    }

    // --- XR9: Resolve indirect references ---

    #[test]
    fn xr9_resolve_reference() {
        let pdf = build_minimal_pdf(
            &[(1, b"<< /Type /Catalog /Pages 2 0 R >>"), (2, b"42")],
            "/Root 1 0 R",
        );

        let table = build_xref_table(&pdf).unwrap();

        // Resolve object 1
        let obj1 = resolve_reference(
            &pdf,
            &table,
            Ref {
                num: 1,
                generation: 0,
            },
            None,
        )
        .unwrap();
        assert_eq!(
            obj1.dict_get(b"Type"),
            Some(&PdfObject::Name(b"Catalog".to_vec()))
        );

        // Resolve object 2
        let obj2 = resolve_reference(
            &pdf,
            &table,
            Ref {
                num: 2,
                generation: 0,
            },
            None,
        )
        .unwrap();
        assert_eq!(obj2, PdfObject::Int(42));
    }

    #[test]
    fn xr9_resolve_deep() {
        let pdf = build_minimal_pdf(
            &[(1, b"<< /Type /Catalog >>"), (2, b"(Hello World)")],
            "/Root 1 0 R",
        );

        let table = build_xref_table(&pdf).unwrap();

        // A Ref should be resolved
        let resolved = resolve_deep(
            &pdf,
            &table,
            &PdfObject::Ref(Ref {
                num: 2,
                generation: 0,
            }),
        )
        .unwrap();
        assert_eq!(resolved, PdfObject::String(b"Hello World".to_vec()));

        // A non-Ref should be returned as-is
        let direct = resolve_deep(&pdf, &table, &PdfObject::Int(99)).unwrap();
        assert_eq!(direct, PdfObject::Int(99));
    }

    #[test]
    fn xr9_resolve_missing_ref() {
        let pdf = build_minimal_pdf(&[(1, b"null")], "");
        let table = build_xref_table(&pdf).unwrap();

        assert!(
            resolve_reference(
                &pdf,
                &table,
                Ref {
                    num: 999,
                    generation: 0
                },
                None
            )
            .is_err()
        );
    }

    // --- Build + resolve end-to-end ---

    #[test]
    fn end_to_end_minimal_pdf() {
        let pdf = build_minimal_pdf(
            &[
                (1, b"<< /Type /Catalog /Pages 2 0 R >>"),
                (2, b"<< /Type /Pages /Kids [] /Count 0 >>"),
            ],
            "/Root 1 0 R",
        );

        let table = build_xref_table(&pdf).unwrap();

        // Trailer has /Root
        let root_ref = table.trailer.dict_get(b"Root").unwrap().as_ref().unwrap();
        assert_eq!(root_ref.num, 1);

        // Resolve root
        let root = resolve_reference(&pdf, &table, root_ref, None).unwrap();
        assert_eq!(
            root.dict_get(b"Type"),
            Some(&PdfObject::Name(b"Catalog".to_vec()))
        );

        // Resolve pages ref from root
        let pages_ref = root.dict_get(b"Pages").unwrap().as_ref().unwrap();
        let pages = resolve_reference(&pdf, &table, pages_ref, None).unwrap();
        assert_eq!(pages.dict_get(b"Count"), Some(&PdfObject::Int(0)));
    }

    // --- Read field helper ---

    #[test]
    fn read_field_widths() {
        assert_eq!(read_field(&[1], 1), 1);
        assert_eq!(read_field(&[0, 100], 2), 100);
        assert_eq!(read_field(&[1, 0], 2), 256);
        assert_eq!(read_field(&[], 0), 0);
    }

    // --- XRefTable methods ---

    #[test]
    fn xref_table_insert_if_absent() {
        let mut table = XRefTable {
            entries: Vec::new(),
            trailer: PdfObject::Null,
        };

        table.insert_if_absent(
            5,
            XRefEntry::Uncompressed {
                offset: 100,
                generation: 0,
            },
        );
        assert_eq!(table.len(), 6); // 0..=5
        assert!(matches!(
            table.get(5),
            Some(XRefEntry::Uncompressed { offset: 100, .. })
        ));

        // Second insert should not overwrite
        table.insert_if_absent(
            5,
            XRefEntry::Uncompressed {
                offset: 200,
                generation: 0,
            },
        );
        assert!(matches!(
            table.get(5),
            Some(XRefEntry::Uncompressed { offset: 100, .. })
        ));
    }
}
