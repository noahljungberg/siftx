//! PDF object parser (PO1-PO7).
//!
//! Parses PDF objects from a token stream per ISO 32000-2 §7.3.

use super::tokenizer::{Keyword, Token, Tokenizer};
use crate::core::{Error, Result};

/// An indirect object reference (object number, generation number).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ref {
    pub num: u32,
    pub generation: u16,
}

/// PO7: Core PDF object type system.
#[derive(Debug, Clone, PartialEq)]
pub enum PdfObject {
    /// Boolean value.
    Bool(bool),
    /// Integer value.
    Int(i64),
    /// Real (floating-point) value.
    Real(f64),
    /// Literal or hex string (decoded bytes).
    String(Vec<u8>),
    /// Name object (decoded bytes, without leading `/`).
    Name(Vec<u8>),
    /// PO1: Array of objects.
    Array(Vec<PdfObject>),
    /// PO2: Dictionary mapping name keys to object values.
    Dict(Vec<(Vec<u8>, PdfObject)>),
    /// PO5: Stream - dictionary + raw byte data.
    Stream {
        dict: Vec<(Vec<u8>, PdfObject)>,
        data: Vec<u8>,
    },
    /// PO4: Indirect reference.
    Ref(Ref),
    /// Null object.
    Null,
}

impl PdfObject {
    /// Get as boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            PdfObject::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as integer.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            PdfObject::Int(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as real, coercing integers.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            PdfObject::Real(v) => Some(*v),
            PdfObject::Int(v) => Some(*v as f64),
            _ => None,
        }
    }

    /// Get as name bytes.
    pub fn as_name(&self) -> Option<&[u8]> {
        match self {
            PdfObject::Name(v) => Some(v),
            _ => None,
        }
    }

    /// Get as name string (UTF-8 lossy).
    pub fn as_name_str(&self) -> Option<&str> {
        match self {
            PdfObject::Name(v) => std::str::from_utf8(v).ok(),
            _ => None,
        }
    }

    /// Get as string bytes.
    pub fn as_string(&self) -> Option<&[u8]> {
        match self {
            PdfObject::String(v) => Some(v),
            _ => None,
        }
    }

    /// Get as array.
    pub fn as_array(&self) -> Option<&[PdfObject]> {
        match self {
            PdfObject::Array(v) => Some(v),
            _ => None,
        }
    }

    /// Get as dictionary.
    pub fn as_dict(&self) -> Option<&[(Vec<u8>, PdfObject)]> {
        match self {
            PdfObject::Dict(v) => Some(v),
            PdfObject::Stream { dict, .. } => Some(dict),
            _ => None,
        }
    }

    /// Get as indirect reference.
    pub fn as_ref(&self) -> Option<Ref> {
        match self {
            PdfObject::Ref(r) => Some(*r),
            _ => None,
        }
    }

    /// Look up a key in a dictionary (or stream dictionary).
    pub fn dict_get(&self, key: &[u8]) -> Option<&PdfObject> {
        self.as_dict()?
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// Get stream data.
    pub fn stream_data(&self) -> Option<&[u8]> {
        match self {
            PdfObject::Stream { data, .. } => Some(data),
            _ => None,
        }
    }

    /// Returns true if this is a null object.
    pub fn is_null(&self) -> bool {
        matches!(self, PdfObject::Null)
    }
}

/// PDF object parser.
///
/// Wraps a `Tokenizer` and provides recursive descent parsing of PDF objects.
/// For stream parsing, it needs access to the raw data to extract stream bytes.
pub struct Parser<'a> {
    tokenizer: Tokenizer<'a>,
    data: &'a [u8],
    /// Maximum nesting depth to prevent stack overflow on malicious input.
    max_depth: u32,
}

impl<'a> Parser<'a> {
    /// Create a new parser over the given bytes.
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            tokenizer: Tokenizer::new(data),
            data,
            max_depth: 256,
        }
    }

    /// Access the underlying tokenizer (e.g. to check/set position).
    pub fn tokenizer(&self) -> &Tokenizer<'a> {
        &self.tokenizer
    }

    /// Access the underlying tokenizer mutably.
    pub fn tokenizer_mut(&mut self) -> &mut Tokenizer<'a> {
        &mut self.tokenizer
    }

    /// Current byte position.
    pub fn position(&self) -> usize {
        self.tokenizer.position()
    }

    /// Set position.
    pub fn seek(&mut self, pos: usize) {
        self.tokenizer.seek(pos);
    }

    /// Parse one PDF object.
    ///
    /// This handles all object types including arrays, dicts, and indirect references.
    /// For indirect object definitions (`N G obj ... endobj`), use `parse_indirect_object`.
    pub fn parse_object(&mut self) -> Result<PdfObject> {
        self.parse_object_depth(0)
    }

    fn parse_object_depth(&mut self, depth: u32) -> Result<PdfObject> {
        if depth > self.max_depth {
            return Err(Error::Format("PDF object nesting too deep".into()));
        }

        // Save position for backtracking (needed for indirect ref detection)
        let saved_pos = self.tokenizer.position();

        let token = self
            .tokenizer
            .next_token()?
            .ok_or_else(|| Error::Format("unexpected end of PDF data".into()))?;

        match token {
            Token::Bool(v) => Ok(PdfObject::Bool(v)),
            Token::Real(v) => Ok(PdfObject::Real(v)),
            Token::Name(v) => Ok(PdfObject::Name(v)),
            Token::LiteralString(v) => Ok(PdfObject::String(v)),
            Token::HexString(v) => Ok(PdfObject::String(v)),
            Token::Keyword(Keyword::Null) => Ok(PdfObject::Null),

            // PO4: Could be an integer or the start of an indirect reference (N G R)
            Token::Int(num) => {
                let after_num_pos = self.tokenizer.position();

                // Try to read generation number + R
                if let Ok(Some(Token::Int(g))) = self.tokenizer.next_token() {
                    let after_gen_pos = self.tokenizer.position();
                    if let Ok(Some(Token::Keyword(Keyword::R))) = self.tokenizer.next_token() {
                        // It's an indirect reference
                        if num >= 0 && g >= 0 && g <= u16::MAX as i64 {
                            return Ok(PdfObject::Ref(Ref {
                                num: num as u32,
                                generation: g as u16,
                            }));
                        }
                    }
                    // Not "N G R" - backtrack past the second token
                    self.tokenizer.seek(after_gen_pos);
                    // Actually we need to backtrack to just after the first int
                    self.tokenizer.seek(after_num_pos);
                }
                // Not enough tokens for a ref, or generation wasn't an int
                // Backtrack to just after the first int was consumed
                self.tokenizer.seek(after_num_pos);
                Ok(PdfObject::Int(num))
            }

            // PO1: Array
            Token::ArrayStart => self.parse_array(depth),

            // PO2: Dictionary (may become stream via PO5)
            Token::DictStart => self.parse_dict_or_stream(depth),

            _ => {
                self.tokenizer.seek(saved_pos);
                Err(Error::Format(format!("unexpected token: {token:?}")))
            }
        }
    }

    /// PO1: Parse array contents after `[`.
    fn parse_array(&mut self, depth: u32) -> Result<PdfObject> {
        let mut items = Vec::new();

        loop {
            // Peek for array end
            let saved = self.tokenizer.position();
            match self.tokenizer.next_token()? {
                Some(Token::ArrayEnd) => return Ok(PdfObject::Array(items)),
                Some(_) => {
                    self.tokenizer.seek(saved);
                    items.push(self.parse_object_depth(depth + 1)?);
                }
                None => return Err(Error::Format("unterminated array".into())),
            }
        }
    }

    /// PO2 + PO5: Parse dictionary contents after `<<`, then check for stream.
    fn parse_dict_or_stream(&mut self, depth: u32) -> Result<PdfObject> {
        let mut entries = Vec::new();

        loop {
            match self.tokenizer.next_token()? {
                Some(Token::DictEnd) => break,
                Some(Token::Name(key)) => {
                    let value = self.parse_object_depth(depth + 1)?;
                    entries.push((key, value));
                }
                Some(tok) => {
                    return Err(Error::Format(format!(
                        "expected name key or >> in dict, got {tok:?}"
                    )));
                }
                None => return Err(Error::Format("unterminated dictionary".into())),
            }
        }

        // PO5: Check if this dict is followed by `stream`
        let after_dict_pos = self.tokenizer.position();
        self.tokenizer.skip_whitespace();
        let peek_pos = self.tokenizer.position();

        // Check for "stream" keyword (don't use tokenizer - stream keyword
        // must be followed immediately by EOL, and the stream data follows)
        if self.data.len() >= peek_pos + 6 && &self.data[peek_pos..peek_pos + 6] == b"stream" {
            // Verify it's terminated (not just a prefix of another word)
            let after_stream = peek_pos + 6;
            if after_stream >= self.data.len()
                || self.data[after_stream] == b'\n'
                || self.data[after_stream] == b'\r'
            {
                let stream_data = self.parse_stream_data(&entries, after_stream)?;
                return Ok(PdfObject::Stream {
                    dict: entries,
                    data: stream_data,
                });
            }
        }

        // Not a stream - restore position to after dict
        self.tokenizer.seek(after_dict_pos);
        Ok(PdfObject::Dict(entries))
    }

    /// PO5 + PO6: Extract stream bytes.
    ///
    /// `keyword_end` is the position just after "stream".
    fn parse_stream_data(
        &mut self,
        dict: &[(Vec<u8>, PdfObject)],
        keyword_end: usize,
    ) -> Result<Vec<u8>> {
        // Skip EOL after "stream": \r\n or \n (spec says \r\n or \n)
        let mut pos = keyword_end;
        if pos < self.data.len() && self.data[pos] == b'\r' {
            pos += 1;
        }
        if pos < self.data.len() && self.data[pos] == b'\n' {
            pos += 1;
        }

        let stream_start = pos;

        // PO5: Try to get length from dictionary
        let length = dict
            .iter()
            .find(|(k, _)| k == b"Length")
            .and_then(|(_, v)| v.as_int())
            .filter(|&len| len >= 0);

        let (stream_end, after_endstream) = if let Some(len) = length {
            let end = stream_start + len as usize;
            if end <= self.data.len() {
                // Verify endstream follows (after optional EOL)
                let mut check = end;
                // Skip optional \r\n or \n before endstream
                if check < self.data.len() && self.data[check] == b'\r' {
                    check += 1;
                }
                if check < self.data.len() && self.data[check] == b'\n' {
                    check += 1;
                }
                if check + 9 <= self.data.len() && &self.data[check..check + 9] == b"endstream" {
                    (end, check + 9)
                } else {
                    // PO6: Length was wrong, fall back to scanning
                    self.scan_for_endstream(stream_start)?
                }
            } else {
                // PO6: Length extends past EOF, scan
                self.scan_for_endstream(stream_start)?
            }
        } else {
            // PO6: No Length (or it's an indirect ref we can't resolve yet), scan
            self.scan_for_endstream(stream_start)?
        };

        let data = self.data[stream_start..stream_end].to_vec();
        self.tokenizer.seek(after_endstream);
        Ok(data)
    }

    /// PO6: Scan for `endstream` keyword when Length is missing or wrong.
    ///
    /// Returns (stream_data_end, position_after_endstream).
    fn scan_for_endstream(&self, start: usize) -> Result<(usize, usize)> {
        // Search for "\nendstream" or "\r\nendstream" or "\rendstream"
        let needle = b"endstream";
        let mut pos = start;
        while pos + needle.len() <= self.data.len() {
            if &self.data[pos..pos + needle.len()] == needle {
                // Found it - determine where stream data actually ends
                // (strip trailing EOL before endstream)
                let mut data_end = pos;
                if data_end > start && self.data[data_end - 1] == b'\n' {
                    data_end -= 1;
                    if data_end > start && self.data[data_end - 1] == b'\r' {
                        data_end -= 1;
                    }
                } else if data_end > start && self.data[data_end - 1] == b'\r' {
                    data_end -= 1;
                }
                return Ok((data_end, pos + needle.len()));
            }
            pos += 1;
        }
        Err(Error::Format("endstream not found".into()))
    }

    /// PO3: Parse an indirect object definition: `N G obj <object> endobj`.
    ///
    /// The tokenizer should be positioned before the object number.
    /// Returns (object_number, generation, parsed_object).
    pub fn parse_indirect_object(&mut self) -> Result<(u32, u16, PdfObject)> {
        // Read "N G obj"
        let num = match self.tokenizer.next_token()? {
            Some(Token::Int(n)) if n >= 0 => n as u32,
            _ => return Err(Error::Format("expected object number".into())),
        };
        let generation = match self.tokenizer.next_token()? {
            Some(Token::Int(g)) if g >= 0 && g <= u16::MAX as i64 => g as u16,
            _ => return Err(Error::Format("expected generation number".into())),
        };
        match self.tokenizer.next_token()? {
            Some(Token::Keyword(Keyword::Obj)) => {}
            _ => return Err(Error::Format("expected 'obj' keyword".into())),
        }

        let obj = self.parse_object()?;

        // Expect "endobj"
        self.tokenizer.skip_whitespace();
        let _saved = self.tokenizer.position();
        match self.tokenizer.next_token()? {
            Some(Token::Keyword(Keyword::EndObj)) => {}
            // For stream objects, endobj follows endstream - already consumed
            // Some PDFs omit endobj after endstream
            _ => {
                self.tokenizer.seek(_saved);
                // Don't error - some PDFs are sloppy
            }
        }

        Ok((num, generation, obj))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &[u8]) -> PdfObject {
        let mut p = Parser::new(input);
        p.parse_object().unwrap()
    }

    // --- PO7: Basic types ---

    #[test]
    fn po7_bool() {
        assert_eq!(parse(b"true"), PdfObject::Bool(true));
        assert_eq!(parse(b"false"), PdfObject::Bool(false));
    }

    #[test]
    fn po7_int() {
        assert_eq!(parse(b"42"), PdfObject::Int(42));
        assert_eq!(parse(b"-7"), PdfObject::Int(-7));
    }

    // 3.14 / 2.718... here are arbitrary floats chosen to have an inexact
    // binary expansion, not approximations of a constant.
    #[allow(clippy::approx_constant)]
    #[test]
    fn po7_real() {
        assert_eq!(parse(b"3.14"), PdfObject::Real(3.14));
    }

    #[test]
    fn po7_null() {
        assert_eq!(parse(b"null"), PdfObject::Null);
        assert!(parse(b"null").is_null());
    }

    #[test]
    fn po7_name() {
        assert_eq!(parse(b"/Type"), PdfObject::Name(b"Type".to_vec()));
    }

    #[test]
    fn po7_literal_string() {
        assert_eq!(parse(b"(hello)"), PdfObject::String(b"hello".to_vec()));
    }

    #[test]
    fn po7_hex_string() {
        assert_eq!(parse(b"<48656C6C6F>"), PdfObject::String(b"Hello".to_vec()));
    }

    // --- PO1: Arrays ---

    #[test]
    fn po1_empty_array() {
        assert_eq!(parse(b"[]"), PdfObject::Array(vec![]));
    }

    #[test]
    fn po1_simple_array() {
        assert_eq!(
            parse(b"[1 2 3]"),
            PdfObject::Array(vec![
                PdfObject::Int(1),
                PdfObject::Int(2),
                PdfObject::Int(3),
            ])
        );
    }

    #[test]
    fn po1_heterogeneous_array() {
        assert_eq!(
            parse(b"[1 (two) /three true null]"),
            PdfObject::Array(vec![
                PdfObject::Int(1),
                PdfObject::String(b"two".to_vec()),
                PdfObject::Name(b"three".to_vec()),
                PdfObject::Bool(true),
                PdfObject::Null,
            ])
        );
    }

    #[test]
    fn po1_nested_array() {
        assert_eq!(
            parse(b"[[1 2] [3 4]]"),
            PdfObject::Array(vec![
                PdfObject::Array(vec![PdfObject::Int(1), PdfObject::Int(2)]),
                PdfObject::Array(vec![PdfObject::Int(3), PdfObject::Int(4)]),
            ])
        );
    }

    #[test]
    fn po1_array_with_refs() {
        assert_eq!(
            parse(b"[1 0 R 2 0 R]"),
            PdfObject::Array(vec![
                PdfObject::Ref(Ref {
                    num: 1,
                    generation: 0
                }),
                PdfObject::Ref(Ref {
                    num: 2,
                    generation: 0
                }),
            ])
        );
    }

    // --- PO2: Dictionaries ---

    #[test]
    fn po2_empty_dict() {
        assert_eq!(parse(b"<< >>"), PdfObject::Dict(vec![]));
    }

    #[test]
    fn po2_simple_dict() {
        let obj = parse(b"<< /Type /Catalog /Pages 3 0 R >>");
        assert_eq!(
            obj,
            PdfObject::Dict(vec![
                (b"Type".to_vec(), PdfObject::Name(b"Catalog".to_vec())),
                (
                    b"Pages".to_vec(),
                    PdfObject::Ref(Ref {
                        num: 3,
                        generation: 0
                    })
                ),
            ])
        );
    }

    #[test]
    fn po2_nested_dict() {
        let obj = parse(b"<< /Info << /Author (John) >> >>");
        assert_eq!(
            obj,
            PdfObject::Dict(vec![(
                b"Info".to_vec(),
                PdfObject::Dict(vec![(
                    b"Author".to_vec(),
                    PdfObject::String(b"John".to_vec()),
                )]),
            )])
        );
    }

    #[test]
    fn po2_dict_get() {
        let obj = parse(b"<< /Type /Catalog /Length 42 >>");
        assert_eq!(
            obj.dict_get(b"Type"),
            Some(&PdfObject::Name(b"Catalog".to_vec()))
        );
        assert_eq!(obj.dict_get(b"Length"), Some(&PdfObject::Int(42)));
        assert_eq!(obj.dict_get(b"Missing"), None);
    }

    // --- PO4: Indirect references ---

    #[test]
    fn po4_indirect_ref() {
        assert_eq!(
            parse(b"10 0 R"),
            PdfObject::Ref(Ref {
                num: 10,
                generation: 0
            })
        );
    }

    #[test]
    fn po4_ref_with_generation() {
        assert_eq!(
            parse(b"5 2 R"),
            PdfObject::Ref(Ref {
                num: 5,
                generation: 2
            })
        );
    }

    #[test]
    fn po4_int_not_ref() {
        // Just an integer, not followed by "G R"
        assert_eq!(parse(b"42"), PdfObject::Int(42));
    }

    #[test]
    fn po4_int_followed_by_name() {
        // "42 /Foo" - 42 is an int, not start of a ref
        let mut p = Parser::new(b"42 /Foo");
        let obj1 = p.parse_object().unwrap();
        let obj2 = p.parse_object().unwrap();
        assert_eq!(obj1, PdfObject::Int(42));
        assert_eq!(obj2, PdfObject::Name(b"Foo".to_vec()));
    }

    // --- PO3: Indirect object definitions ---

    #[test]
    fn po3_indirect_object() {
        let mut p = Parser::new(b"1 0 obj\n42\nendobj");
        let (num, generation, obj) = p.parse_indirect_object().unwrap();
        assert_eq!(num, 1);
        assert_eq!(generation, 0);
        assert_eq!(obj, PdfObject::Int(42));
    }

    #[test]
    fn po3_indirect_dict() {
        let input = b"5 0 obj\n<< /Type /Page /Parent 3 0 R >>\nendobj";
        let mut p = Parser::new(input);
        let (num, generation, obj) = p.parse_indirect_object().unwrap();
        assert_eq!(num, 5);
        assert_eq!(generation, 0);
        assert_eq!(
            obj,
            PdfObject::Dict(vec![
                (b"Type".to_vec(), PdfObject::Name(b"Page".to_vec())),
                (
                    b"Parent".to_vec(),
                    PdfObject::Ref(Ref {
                        num: 3,
                        generation: 0
                    })
                ),
            ])
        );
    }

    // --- PO5: Streams ---

    #[test]
    fn po5_stream_with_length() {
        let input = b"<< /Length 5 >>\nstream\nHello\nendstream";
        let obj = parse(input);
        match &obj {
            PdfObject::Stream { dict, data } => {
                assert_eq!(dict.len(), 1);
                assert_eq!(data, b"Hello");
            }
            _ => panic!("expected Stream, got {obj:?}"),
        }
    }

    #[test]
    fn po5_stream_crlf() {
        let input = b"<< /Length 5 >>\r\nstream\r\nHello\r\nendstream";
        let obj = parse(input);
        assert_eq!(obj.stream_data().unwrap(), b"Hello");
    }

    #[test]
    fn po5_stream_in_indirect() {
        let input = b"1 0 obj\n<< /Length 3 >>\nstream\nABC\nendstream\nendobj";
        let mut p = Parser::new(input);
        let (num, generation, obj) = p.parse_indirect_object().unwrap();
        assert_eq!(num, 1);
        assert_eq!(generation, 0);
        assert_eq!(obj.stream_data().unwrap(), b"ABC");
    }

    #[test]
    fn po5_stream_dict_access() {
        let input = b"<< /Length 5 /Filter /FlateDecode >>\nstream\nHello\nendstream";
        let obj = parse(input);
        // dict_get works on streams too
        assert_eq!(
            obj.dict_get(b"Filter"),
            Some(&PdfObject::Name(b"FlateDecode".to_vec()))
        );
    }

    // --- PO6: Stream with wrong/missing length ---

    #[test]
    fn po6_stream_wrong_length() {
        // Length says 100 but actual data is "Hello" (5 bytes)
        let input = b"<< /Length 100 >>\nstream\nHello\nendstream";
        let obj = parse(input);
        assert_eq!(obj.stream_data().unwrap(), b"Hello");
    }

    #[test]
    fn po6_stream_no_length() {
        // No Length key at all - must scan for endstream
        let input = b"<< >>\nstream\nSomeData\nendstream";
        let obj = parse(input);
        assert_eq!(obj.stream_data().unwrap(), b"SomeData");
    }

    #[test]
    fn po6_stream_length_is_ref() {
        // Length is an indirect ref - can't resolve it at parse time, must scan
        let input = b"<< /Length 10 0 R >>\nstream\nXYZ\nendstream";
        let obj = parse(input);
        assert_eq!(obj.stream_data().unwrap(), b"XYZ");
    }

    // --- Accessor methods ---

    #[test]
    fn accessor_methods() {
        assert_eq!(PdfObject::Bool(true).as_bool(), Some(true));
        assert_eq!(PdfObject::Int(42).as_int(), Some(42));
        assert_eq!(PdfObject::Real(1.5).as_f64(), Some(1.5));
        assert_eq!(PdfObject::Int(10).as_f64(), Some(10.0));
        assert_eq!(PdfObject::Name(b"Foo".to_vec()).as_name_str(), Some("Foo"));
        assert_eq!(
            PdfObject::String(b"bar".to_vec()).as_string(),
            Some(b"bar".as_ref())
        );
        assert_eq!(
            PdfObject::Ref(Ref {
                num: 1,
                generation: 0
            })
            .as_ref(),
            Some(Ref {
                num: 1,
                generation: 0
            })
        );
        assert!(PdfObject::Null.is_null());
    }

    // --- Nesting depth limit ---

    #[test]
    fn depth_limit() {
        // Build deeply nested array: [[[[...]]]]
        let mut input = Vec::new();
        for _ in 0..300 {
            input.push(b'[');
        }
        input.extend_from_slice(b"1");
        for _ in 0..300 {
            input.push(b']');
        }
        let mut p = Parser::new(&input);
        assert!(p.parse_object().is_err());
    }
}
