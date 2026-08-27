//! PDF tokenizer (PT1-PT6).
//!
//! Tokenizes raw PDF bytes into a stream of typed tokens per ISO 32000-2 §7.2-7.3.

use crate::core::Error;
use crate::core::Result;

/// PDF whitespace bytes (ISO 32000-2 §7.2.2).
const fn is_whitespace(b: u8) -> bool {
    matches!(b, 0 | 9 | 10 | 12 | 13 | 32)
}

/// PDF delimiter bytes (ISO 32000-2 §7.2.2).
const fn is_delimiter(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

/// Whether a byte terminates a token (whitespace or delimiter).
const fn is_token_end(b: u8) -> bool {
    is_whitespace(b) || is_delimiter(b)
}

/// A PDF keyword token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Obj,
    EndObj,
    Stream,
    EndStream,
    Xref,
    Trailer,
    StartXref,
    /// Indirect reference operator.
    R,
    Null,
}

/// A single PDF token.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// `true` or `false`.
    Bool(bool),
    /// Integer number.
    Int(i64),
    /// Real (floating-point) number.
    Real(f64),
    /// Name object (e.g. `/Type`). Stored as raw bytes with `#xx` escapes decoded.
    Name(Vec<u8>),
    /// Literal string `(...)` with escapes resolved.
    LiteralString(Vec<u8>),
    /// Hex string `<...>` decoded to bytes.
    HexString(Vec<u8>),
    /// A PDF keyword.
    Keyword(Keyword),
    /// `[`
    ArrayStart,
    /// `]`
    ArrayEnd,
    /// `<<`
    DictStart,
    /// `>>`
    DictEnd,
}

/// PDF tokenizer operating over a byte slice.
pub struct Tokenizer<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    /// Create a new tokenizer over the given bytes.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Current byte position in the input.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Set the position.
    pub fn seek(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Remaining bytes.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Peek at the current byte without advancing.
    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    /// Peek at the byte at pos + offset.
    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.data.get(self.pos + offset).copied()
    }

    /// Advance by one byte and return it.
    fn advance(&mut self) -> Option<u8> {
        let b = self.data.get(self.pos).copied()?;
        self.pos += 1;
        Some(b)
    }

    // --- PT1: Skip whitespace and comments ---

    /// Skip all whitespace and comments. Returns the number of bytes skipped.
    pub fn skip_whitespace(&mut self) -> usize {
        let start = self.pos;
        loop {
            match self.peek() {
                Some(b) if is_whitespace(b) => {
                    self.pos += 1;
                }
                Some(b'%') => self.skip_comment(),
                _ => break,
            }
        }
        self.pos - start
    }

    /// Skip a comment from `%` through end of line.
    fn skip_comment(&mut self) {
        self.pos += 1; // skip '%'
        while let Some(b) = self.peek() {
            self.pos += 1;
            if b == b'\r' || b == b'\n' {
                // CR+LF counts as single EOL
                if b == b'\r' && self.peek() == Some(b'\n') {
                    self.pos += 1;
                }
                break;
            }
        }
    }

    // --- Main tokenizer entry point ---

    /// Read the next token, or `None` at EOF.
    pub fn next_token(&mut self) -> Result<Option<Token>> {
        self.skip_whitespace();

        let Some(b) = self.peek() else {
            return Ok(None);
        };

        match b {
            b'[' => {
                self.pos += 1;
                Ok(Some(Token::ArrayStart))
            }
            b']' => {
                self.pos += 1;
                Ok(Some(Token::ArrayEnd))
            }

            // PT4: Name
            b'/' => self.read_name().map(Some),

            // PT5: Literal string
            b'(' => self.read_literal_string().map(Some),

            // PT6: Hex string or dict delimiter
            b'<' => {
                if self.peek_at(1) == Some(b'<') {
                    self.pos += 2;
                    Ok(Some(Token::DictStart))
                } else {
                    self.read_hex_string().map(Some)
                }
            }

            b'>' => {
                if self.peek_at(1) == Some(b'>') {
                    self.pos += 2;
                    Ok(Some(Token::DictEnd))
                } else {
                    Err(Error::Format("unexpected '>'".into()))
                }
            }

            // PT3: Number (digit, sign, or decimal point)
            b'+' | b'-' | b'.' | b'0'..=b'9' => self.read_number().map(Some),

            // PT2: Keyword or bool
            _ => self.read_keyword_or_bool().map(Some),
        }
    }

    // --- PT3: Numbers ---

    fn read_number(&mut self) -> Result<Token> {
        let start = self.pos;
        let mut has_dot = false;

        // Optional sign
        if matches!(self.peek(), Some(b'+') | Some(b'-')) {
            self.pos += 1;
        }

        // Digits and optional decimal point
        loop {
            match self.peek() {
                Some(b'0'..=b'9') => {
                    self.pos += 1;
                }
                Some(b'.') if !has_dot => {
                    has_dot = true;
                    self.pos += 1;
                }
                _ => break,
            }
        }

        let num_str = std::str::from_utf8(&self.data[start..self.pos])
            .map_err(|_| Error::Format("invalid number bytes".into()))?;

        if has_dot {
            let v: f64 = num_str
                .parse()
                .map_err(|_| Error::Format(format!("invalid real number: {num_str}")))?;
            Ok(Token::Real(v))
        } else {
            let v: i64 = num_str
                .parse()
                .map_err(|_| Error::Format(format!("invalid integer: {num_str}")))?;
            Ok(Token::Int(v))
        }
    }

    // --- PT4: Name objects ---

    fn read_name(&mut self) -> Result<Token> {
        self.pos += 1; // skip '/'

        let mut name = Vec::new();
        loop {
            match self.peek() {
                None => break,
                Some(b) if is_whitespace(b) || is_delimiter(b) => break,
                Some(b'#') => {
                    self.pos += 1;
                    let hi = self
                        .advance()
                        .ok_or_else(|| Error::Format("truncated name hex escape".into()))?;
                    let lo = self
                        .advance()
                        .ok_or_else(|| Error::Format("truncated name hex escape".into()))?;
                    let byte = hex_digit(hi)? << 4 | hex_digit(lo)?;
                    name.push(byte);
                }
                Some(b) => {
                    name.push(b);
                    self.pos += 1;
                }
            }
        }

        Ok(Token::Name(name))
    }

    // --- PT5: Literal strings ---

    fn read_literal_string(&mut self) -> Result<Token> {
        self.pos += 1; // skip '('
        let mut result = Vec::new();
        let mut depth: u32 = 1;

        loop {
            let b = self
                .advance()
                .ok_or_else(|| Error::Format("unterminated literal string".into()))?;

            match b {
                b'(' => {
                    depth += 1;
                    result.push(b'(');
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    result.push(b')');
                }
                b'\\' => {
                    let esc = self
                        .advance()
                        .ok_or_else(|| Error::Format("unterminated escape in string".into()))?;
                    match esc {
                        b'n' => result.push(b'\n'),
                        b'r' => result.push(b'\r'),
                        b't' => result.push(b'\t'),
                        b'b' => result.push(0x08),
                        b'f' => result.push(0x0C),
                        b'(' => result.push(b'('),
                        b')' => result.push(b')'),
                        b'\\' => result.push(b'\\'),
                        // Line continuation: backslash + EOL is ignored
                        b'\r' => {
                            // \r\n also counts as single EOL
                            if self.peek() == Some(b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => { /* ignore */ }
                        // Octal escape: 1-3 octal digits
                        b'0'..=b'7' => {
                            let mut val = (esc - b'0') as u16;
                            if matches!(self.peek(), Some(b'0'..=b'7')) {
                                val = val * 8 + (self.advance().unwrap() - b'0') as u16;
                                if matches!(self.peek(), Some(b'0'..=b'7')) {
                                    val = val * 8 + (self.advance().unwrap() - b'0') as u16;
                                }
                            }
                            result.push((val & 0xFF) as u8);
                        }
                        // Unknown escape: ignore the backslash per spec
                        _ => result.push(esc),
                    }
                }
                _ => result.push(b),
            }
        }

        Ok(Token::LiteralString(result))
    }

    // --- PT6: Hex strings ---

    fn read_hex_string(&mut self) -> Result<Token> {
        self.pos += 1; // skip '<'
        let mut nibbles = Vec::new();

        loop {
            let b = self
                .advance()
                .ok_or_else(|| Error::Format("unterminated hex string".into()))?;

            match b {
                b'>' => break,
                _ if is_whitespace(b) => continue,
                _ => {
                    nibbles.push(hex_digit(b)?);
                }
            }
        }

        // Odd nibble count: append 0
        if nibbles.len() % 2 != 0 {
            nibbles.push(0);
        }

        let bytes: Vec<u8> = nibbles
            .chunks(2)
            .map(|pair| pair[0] << 4 | pair[1])
            .collect();

        Ok(Token::HexString(bytes))
    }

    // --- PT2: Keywords and booleans ---

    fn read_keyword_or_bool(&mut self) -> Result<Token> {
        let start = self.pos;

        // Read until whitespace or delimiter
        while let Some(b) = self.peek() {
            if is_token_end(b) {
                break;
            }
            self.pos += 1;
        }

        if self.pos == start {
            return Err(Error::Format(format!(
                "unexpected byte 0x{:02X} at offset {}",
                self.data[start], start
            )));
        }

        let word = &self.data[start..self.pos];

        match word {
            b"true" => Ok(Token::Bool(true)),
            b"false" => Ok(Token::Bool(false)),
            b"null" => Ok(Token::Keyword(Keyword::Null)),
            b"obj" => Ok(Token::Keyword(Keyword::Obj)),
            b"endobj" => Ok(Token::Keyword(Keyword::EndObj)),
            b"stream" => Ok(Token::Keyword(Keyword::Stream)),
            b"endstream" => Ok(Token::Keyword(Keyword::EndStream)),
            b"xref" => Ok(Token::Keyword(Keyword::Xref)),
            b"trailer" => Ok(Token::Keyword(Keyword::Trailer)),
            b"startxref" => Ok(Token::Keyword(Keyword::StartXref)),
            b"R" => Ok(Token::Keyword(Keyword::R)),
            _ => {
                let s = String::from_utf8_lossy(word);
                Err(Error::Format(format!("unknown token: {s}")))
            }
        }
    }
}

/// Decode a single hex digit (case-insensitive).
fn hex_digit(b: u8) -> Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(Error::Format(format!("invalid hex digit: 0x{b:02X}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(input: &[u8]) -> Vec<Token> {
        let mut t = Tokenizer::new(input);
        let mut tokens = Vec::new();
        while let Ok(Some(tok)) = t.next_token() {
            tokens.push(tok);
        }
        tokens
    }

    fn tokenize_one(input: &[u8]) -> Token {
        let mut t = Tokenizer::new(input);
        t.next_token().unwrap().unwrap()
    }

    // --- PT1: Whitespace and comments ---

    #[test]
    fn pt1_skip_whitespace() {
        let mut t = Tokenizer::new(b"   \t\n\r\x0C\x00 42");
        t.skip_whitespace();
        assert_eq!(t.position(), 9);
    }

    #[test]
    fn pt1_skip_comment() {
        let toks = tokenize(b"% this is a comment\n42");
        assert_eq!(toks, vec![Token::Int(42)]);
    }

    #[test]
    fn pt1_skip_comment_cr() {
        let toks = tokenize(b"% comment\r42");
        assert_eq!(toks, vec![Token::Int(42)]);
    }

    #[test]
    fn pt1_skip_comment_crlf() {
        let toks = tokenize(b"% comment\r\n42");
        assert_eq!(toks, vec![Token::Int(42)]);
    }

    #[test]
    fn pt1_multiple_comments() {
        let toks = tokenize(b"% first\n% second\n true");
        assert_eq!(toks, vec![Token::Bool(true)]);
    }

    #[test]
    fn pt1_comment_at_eof() {
        let toks = tokenize(b"% comment at eof");
        assert!(toks.is_empty());
    }

    // --- PT2: Keywords ---

    #[test]
    fn pt2_keywords() {
        let cases: &[(&[u8], Keyword)] = &[
            (b"obj", Keyword::Obj),
            (b"endobj", Keyword::EndObj),
            (b"stream", Keyword::Stream),
            (b"endstream", Keyword::EndStream),
            (b"xref", Keyword::Xref),
            (b"trailer", Keyword::Trailer),
            (b"startxref", Keyword::StartXref),
            (b"R", Keyword::R),
            (b"null", Keyword::Null),
        ];
        for &(input, kw) in cases {
            assert_eq!(
                tokenize_one(input),
                Token::Keyword(kw),
                "failed for {:?}",
                std::str::from_utf8(input)
            );
        }
    }

    #[test]
    fn pt2_booleans() {
        assert_eq!(tokenize_one(b"true"), Token::Bool(true));
        assert_eq!(tokenize_one(b"false"), Token::Bool(false));
    }

    #[test]
    fn pt2_keyword_terminated_by_delimiter() {
        let toks = tokenize(b"obj/Type");
        assert_eq!(toks.len(), 2);
        assert_eq!(toks[0], Token::Keyword(Keyword::Obj));
        assert_eq!(toks[1], Token::Name(b"Type".to_vec()));
    }

    #[test]
    fn pt2_keyword_terminated_by_whitespace() {
        let toks = tokenize(b"obj 1");
        assert_eq!(toks, vec![Token::Keyword(Keyword::Obj), Token::Int(1)]);
    }

    // --- PT3: Numbers ---

    #[test]
    fn pt3_integers() {
        assert_eq!(tokenize_one(b"42"), Token::Int(42));
        assert_eq!(tokenize_one(b"-17"), Token::Int(-17));
        assert_eq!(tokenize_one(b"+5"), Token::Int(5));
        assert_eq!(tokenize_one(b"0"), Token::Int(0));
    }

    // 3.14 / 2.718... here are arbitrary floats chosen to have an inexact
    // binary expansion, not approximations of a constant.
    #[allow(clippy::approx_constant)]
    #[test]
    fn pt3_reals() {
        assert_eq!(tokenize_one(b"3.14"), Token::Real(3.14));
        assert_eq!(tokenize_one(b"-2.5"), Token::Real(-2.5));
        assert_eq!(tokenize_one(b"+0.1"), Token::Real(0.1));
    }

    #[test]
    fn pt3_real_no_leading_zero() {
        assert_eq!(tokenize_one(b".5"), Token::Real(0.5));
        assert_eq!(tokenize_one(b"-.75"), Token::Real(-0.75));
    }

    #[test]
    fn pt3_real_trailing_dot() {
        assert_eq!(tokenize_one(b"5."), Token::Real(5.0));
    }

    #[test]
    fn pt3_number_sequence() {
        let toks = tokenize(b"1 2.5 -3");
        assert_eq!(toks, vec![Token::Int(1), Token::Real(2.5), Token::Int(-3)]);
    }

    // --- PT4: Names ---

    #[test]
    fn pt4_simple_name() {
        assert_eq!(tokenize_one(b"/Type"), Token::Name(b"Type".to_vec()));
        assert_eq!(tokenize_one(b"/Length"), Token::Name(b"Length".to_vec()));
    }

    #[test]
    fn pt4_empty_name() {
        // The empty name "/" is valid per spec
        assert_eq!(tokenize_one(b"/"), Token::Name(vec![]));
    }

    #[test]
    fn pt4_name_hex_escape() {
        // /Name#20With#20Spaces -> "Name With Spaces"
        assert_eq!(
            tokenize_one(b"/Name#20With#20Spaces"),
            Token::Name(b"Name With Spaces".to_vec())
        );
    }

    #[test]
    fn pt4_name_terminated_by_delimiter() {
        let toks = tokenize(b"/Type/Catalog");
        assert_eq!(toks.len(), 2);
        assert_eq!(toks[0], Token::Name(b"Type".to_vec()));
        assert_eq!(toks[1], Token::Name(b"Catalog".to_vec()));
    }

    #[test]
    fn pt4_name_terminated_by_whitespace() {
        let toks = tokenize(b"/Type 42");
        assert_eq!(toks.len(), 2);
        assert_eq!(toks[0], Token::Name(b"Type".to_vec()));
        assert_eq!(toks[1], Token::Int(42));
    }

    // --- PT5: Literal strings ---

    #[test]
    fn pt5_simple_string() {
        assert_eq!(
            tokenize_one(b"(Hello World)"),
            Token::LiteralString(b"Hello World".to_vec())
        );
    }

    #[test]
    fn pt5_empty_string() {
        assert_eq!(tokenize_one(b"()"), Token::LiteralString(vec![]));
    }

    #[test]
    fn pt5_nested_parens() {
        assert_eq!(
            tokenize_one(b"(balanced (parens) here)"),
            Token::LiteralString(b"balanced (parens) here".to_vec())
        );
    }

    #[test]
    fn pt5_deeply_nested_parens() {
        assert_eq!(
            tokenize_one(b"(a(b(c)d)e)"),
            Token::LiteralString(b"a(b(c)d)e".to_vec())
        );
    }

    #[test]
    fn pt5_backslash_escapes() {
        assert_eq!(
            tokenize_one(b"(\\n\\r\\t\\b\\f\\\\\\(\\))"),
            Token::LiteralString(vec![b'\n', b'\r', b'\t', 0x08, 0x0C, b'\\', b'(', b')'])
        );
    }

    #[test]
    fn pt5_octal_escape() {
        // \101 = 'A' (65 decimal)
        assert_eq!(
            tokenize_one(b"(\\101)"),
            Token::LiteralString(b"A".to_vec())
        );
    }

    #[test]
    fn pt5_octal_one_digit() {
        // \0 = NUL
        assert_eq!(tokenize_one(b"(\\0)"), Token::LiteralString(vec![0]));
    }

    #[test]
    fn pt5_octal_two_digits() {
        // \12 = 10 = LF
        assert_eq!(tokenize_one(b"(\\12)"), Token::LiteralString(vec![10]));
    }

    #[test]
    fn pt5_line_continuation_lf() {
        assert_eq!(
            tokenize_one(b"(abc\\\ndef)"),
            Token::LiteralString(b"abcdef".to_vec())
        );
    }

    #[test]
    fn pt5_line_continuation_cr() {
        assert_eq!(
            tokenize_one(b"(abc\\\rdef)"),
            Token::LiteralString(b"abcdef".to_vec())
        );
    }

    #[test]
    fn pt5_line_continuation_crlf() {
        assert_eq!(
            tokenize_one(b"(abc\\\r\ndef)"),
            Token::LiteralString(b"abcdef".to_vec())
        );
    }

    #[test]
    fn pt5_unknown_escape_ignored() {
        // \q -> q (backslash ignored for unknown escapes per spec)
        assert_eq!(tokenize_one(b"(\\q)"), Token::LiteralString(b"q".to_vec()));
    }

    // --- PT6: Hex strings ---

    #[test]
    fn pt6_simple_hex() {
        assert_eq!(
            tokenize_one(b"<48656C6C6F>"),
            Token::HexString(b"Hello".to_vec())
        );
    }

    #[test]
    fn pt6_empty_hex() {
        assert_eq!(tokenize_one(b"<>"), Token::HexString(vec![]));
    }

    #[test]
    fn pt6_hex_with_whitespace() {
        assert_eq!(
            tokenize_one(b"<48 65 6C\n6C 6F>"),
            Token::HexString(b"Hello".to_vec())
        );
    }

    #[test]
    fn pt6_hex_odd_nibble() {
        // <ABC> -> odd, append 0 -> AB C0
        assert_eq!(tokenize_one(b"<ABC>"), Token::HexString(vec![0xAB, 0xC0]));
    }

    #[test]
    fn pt6_hex_lowercase() {
        assert_eq!(tokenize_one(b"<4a4b>"), Token::HexString(vec![0x4A, 0x4B]));
    }

    // --- Delimiter tokens ---

    #[test]
    fn array_delimiters() {
        let toks = tokenize(b"[1 2 3]");
        assert_eq!(
            toks,
            vec![
                Token::ArrayStart,
                Token::Int(1),
                Token::Int(2),
                Token::Int(3),
                Token::ArrayEnd,
            ]
        );
    }

    #[test]
    fn dict_delimiters() {
        let toks = tokenize(b"<< /Type /Catalog >>");
        assert_eq!(
            toks,
            vec![
                Token::DictStart,
                Token::Name(b"Type".to_vec()),
                Token::Name(b"Catalog".to_vec()),
                Token::DictEnd,
            ]
        );
    }

    // --- Mixed token sequences ---

    #[test]
    fn mixed_sequence() {
        let toks = tokenize(b"1 0 obj << /Length 5 >> stream");
        assert_eq!(
            toks,
            vec![
                Token::Int(1),
                Token::Int(0),
                Token::Keyword(Keyword::Obj),
                Token::DictStart,
                Token::Name(b"Length".to_vec()),
                Token::Int(5),
                Token::DictEnd,
                Token::Keyword(Keyword::Stream),
            ]
        );
    }

    #[test]
    fn indirect_reference() {
        let toks = tokenize(b"10 0 R");
        assert_eq!(
            toks,
            vec![Token::Int(10), Token::Int(0), Token::Keyword(Keyword::R)]
        );
    }

    #[test]
    fn empty_input() {
        let toks = tokenize(b"");
        assert!(toks.is_empty());
    }

    #[test]
    fn whitespace_only() {
        let toks = tokenize(b"   \t\n\r  ");
        assert!(toks.is_empty());
    }

    #[test]
    fn pdf_header_comment() {
        let toks = tokenize(b"%PDF-1.7\n1 0 obj");
        assert_eq!(
            toks,
            vec![Token::Int(1), Token::Int(0), Token::Keyword(Keyword::Obj),]
        );
    }

    #[test]
    fn tokenizer_position_tracking() {
        let mut t = Tokenizer::new(b"  42  true");
        let tok1 = t.next_token().unwrap().unwrap();
        assert_eq!(tok1, Token::Int(42));
        assert_eq!(t.position(), 4);
        let tok2 = t.next_token().unwrap().unwrap();
        assert_eq!(tok2, Token::Bool(true));
        assert_eq!(t.position(), 10);
    }
}
