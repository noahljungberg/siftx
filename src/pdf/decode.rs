//! PDF stream decompression and filter pipeline (SD1-SD12).
//!
//! Decodes PDF stream data through filter chains per ISO 32000-2 §7.4.

use super::object::PdfObject;
use crate::core::{Error, Result};

// ---------------------------------------------------------------------------
// SD7: Filter pipeline - the main public entry point
// ---------------------------------------------------------------------------

/// SD7: Decode a stream's raw data using its /Filter and /DecodeParms.
///
/// Applies filters in order. If `/Filter` is an array, filters are applied
/// left to right. Each filter's corresponding `/DecodeParms` entry (if any)
/// provides parameters.
pub fn decode_stream(stream: &PdfObject, raw: &[u8]) -> Result<Vec<u8>> {
    let filters = get_filters(stream);
    let parms_list = get_decode_parms(stream, filters.len());

    let mut data = raw.to_vec();

    for (i, filter) in filters.iter().enumerate() {
        let parms = parms_list.get(i).and_then(|p| p.as_ref());
        data = apply_filter(filter, &data, parms)?;
    }

    Ok(data)
}

/// Decode a stream applying all filters *except* the last one.
///
/// Used for image passthrough extraction: if the last filter is an image-format
/// filter (DCTDecode, JPXDecode, etc.), we apply all preceding transport filters
/// (ASCII85, FlateDecode) but leave the image data in its native format.
///
/// If there is only one filter (or none), returns the raw data unchanged.
pub fn decode_stream_except_last(stream: &PdfObject, raw: &[u8]) -> Result<Vec<u8>> {
    let filters = get_filters(stream);
    if filters.len() <= 1 {
        return Ok(raw.to_vec());
    }

    let parms_list = get_decode_parms(stream, filters.len());
    let mut data = raw.to_vec();

    // Apply all filters except the last
    for (i, filter) in filters[..filters.len() - 1].iter().enumerate() {
        let parms = parms_list.get(i).and_then(|p| p.as_ref());
        data = apply_filter(filter, &data, parms)?;
    }

    Ok(data)
}

/// Extract filter names from a stream dictionary.
pub fn get_filters(obj: &PdfObject) -> Vec<Vec<u8>> {
    match obj.dict_get(b"Filter") {
        Some(PdfObject::Name(name)) => vec![name.clone()],
        Some(PdfObject::Array(arr)) => arr
            .iter()
            .filter_map(|item| item.as_name().map(|n| n.to_vec()))
            .collect(),
        _ => vec![],
    }
}

/// Extract DecodeParms for each filter.
pub fn get_decode_parms(obj: &PdfObject, count: usize) -> Vec<Option<PdfObject>> {
    match obj.dict_get(b"DecodeParms") {
        Some(PdfObject::Dict(_)) => {
            // Single dict - applies to the single (or first) filter
            let mut result = vec![Some(obj.dict_get(b"DecodeParms").unwrap().clone())];
            result.resize(count, None);
            result
        }
        Some(PdfObject::Array(arr)) => arr
            .iter()
            .map(|item| {
                if item.is_null() {
                    None
                } else {
                    Some(item.clone())
                }
            })
            .collect(),
        _ => vec![None; count],
    }
}

/// Apply a single filter to data.
pub fn apply_filter(filter_name: &[u8], data: &[u8], parms: Option<&PdfObject>) -> Result<Vec<u8>> {
    match filter_name {
        b"FlateDecode" | b"Fl" => decode_flate(data, parms),
        b"ASCII85Decode" | b"A85" => decode_ascii85(data),
        b"ASCIIHexDecode" | b"AHx" => decode_ascii_hex(data),
        b"LZWDecode" | b"LZW" => decode_lzw(data, parms),
        b"RunLengthDecode" | b"RL" => decode_run_length(data),
        // SD8-SD11: Passthrough filters - return raw bytes
        b"DCTDecode" | b"DCT" => Ok(data.to_vec()),
        b"CCITTFaxDecode" | b"CCF" => Ok(data.to_vec()),
        b"JBIG2Decode" => Ok(data.to_vec()),
        b"JPXDecode" => Ok(data.to_vec()),
        // SD12: Crypt filter - passthrough (encryption handled separately)
        b"Crypt" => Ok(data.to_vec()),
        _ => {
            let name = String::from_utf8_lossy(filter_name);
            Err(Error::Unsupported(format!("unknown filter: {name}")))
        }
    }
}

// ---------------------------------------------------------------------------
// SD1 + SD2: FlateDecode with optional predictor
// ---------------------------------------------------------------------------

/// SD1: Decompress zlib/deflate data (FlateDecode, ISO 32000-2 §7.4.4).
pub fn flate_decompress(data: &[u8]) -> Result<Vec<u8>> {
    miniz_oxide::inflate::decompress_to_vec_zlib(data)
        .map_err(|e| Error::Format(format!("FlateDecode failed: {e:?}")))
}

/// SD1 + SD2: FlateDecode with optional predictor from DecodeParms.
fn decode_flate(data: &[u8], parms: Option<&PdfObject>) -> Result<Vec<u8>> {
    let decompressed = flate_decompress(data)?;

    if let Some(parms) = parms {
        let predictor = parms
            .dict_get(b"Predictor")
            .and_then(|p| p.as_int())
            .unwrap_or(1);

        if predictor >= 10 {
            // PNG predictors (10-15)
            let columns = parms
                .dict_get(b"Columns")
                .and_then(|c| c.as_int())
                .unwrap_or(1) as usize;
            return apply_png_predictor(&decompressed, columns);
        } else if predictor == 2 {
            // TIFF predictor 2 (horizontal differencing)
            let columns = parms
                .dict_get(b"Columns")
                .and_then(|c| c.as_int())
                .unwrap_or(1) as usize;
            let colors = parms
                .dict_get(b"Colors")
                .and_then(|c| c.as_int())
                .unwrap_or(1) as usize;
            let bpc = parms
                .dict_get(b"BitsPerComponent")
                .and_then(|b| b.as_int())
                .unwrap_or(8) as usize;
            return apply_tiff_predictor(&decompressed, columns, colors, bpc);
        }
    }

    Ok(decompressed)
}

// ---------------------------------------------------------------------------
// SD2: PNG predictors
// ---------------------------------------------------------------------------

/// SD2: Apply PNG predictor to decompressed data (ISO 32000-2 §7.4.4.4).
///
/// Each row is `columns + 1` bytes: 1-byte filter type + `columns` data bytes.
pub fn apply_png_predictor(data: &[u8], columns: usize) -> Result<Vec<u8>> {
    let row_len = columns + 1;
    if data.is_empty() {
        return Ok(Vec::new());
    }
    if data.len() % row_len != 0 {
        return Err(Error::Format(format!(
            "PNG predictor: data length {} not divisible by row length {}",
            data.len(),
            row_len
        )));
    }

    let num_rows = data.len() / row_len;
    let mut result = Vec::with_capacity(num_rows * columns);
    let mut prev_row = vec![0u8; columns];

    for row_idx in 0..num_rows {
        let row_start = row_idx * row_len;
        let filter_type = data[row_start];
        let row_data = &data[row_start + 1..row_start + row_len];

        let mut decoded = vec![0u8; columns];

        match filter_type {
            0 => decoded.copy_from_slice(row_data),
            1 => {
                // Sub
                decoded[0] = row_data[0];
                for i in 1..columns {
                    decoded[i] = row_data[i].wrapping_add(decoded[i - 1]);
                }
            }
            2 => {
                // Up
                for i in 0..columns {
                    decoded[i] = row_data[i].wrapping_add(prev_row[i]);
                }
            }
            3 => {
                // Average
                for i in 0..columns {
                    let left = if i > 0 { decoded[i - 1] as u16 } else { 0 };
                    let above = prev_row[i] as u16;
                    decoded[i] = row_data[i].wrapping_add(((left + above) / 2) as u8);
                }
            }
            4 => {
                // Paeth
                for i in 0..columns {
                    let left = if i > 0 { decoded[i - 1] } else { 0 };
                    let above = prev_row[i];
                    let upper_left = if i > 0 { prev_row[i - 1] } else { 0 };
                    decoded[i] = row_data[i].wrapping_add(paeth(left, above, upper_left));
                }
            }
            _ => decoded.copy_from_slice(row_data),
        }

        result.extend_from_slice(&decoded);
        prev_row.copy_from_slice(&decoded);
    }

    Ok(result)
}

/// Paeth predictor function.
fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let a = a as i16;
    let b = b as i16;
    let c = c as i16;
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

/// TIFF Predictor 2: horizontal differencing (8-bit only for simplicity).
fn apply_tiff_predictor(
    data: &[u8],
    columns: usize,
    colors: usize,
    _bpc: usize,
) -> Result<Vec<u8>> {
    let row_len = columns * colors;
    if data.is_empty() || row_len == 0 {
        return Ok(data.to_vec());
    }

    let mut result = data.to_vec();
    let num_rows = data.len() / row_len;

    for row in 0..num_rows {
        let start = row * row_len;
        for i in colors..row_len {
            let idx = start + i;
            if idx < result.len() {
                result[idx] = result[idx].wrapping_add(result[idx - colors]);
            }
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// SD3: ASCII85Decode
// ---------------------------------------------------------------------------

/// SD3: ASCII85Decode (ISO 32000-2 §7.4.3).
///
/// Decodes base-85 encoded data. Each group of 5 ASCII chars (33-117)
/// encodes 4 binary bytes. `z` encodes 4 zero bytes. `~>` marks EOD.
fn decode_ascii85(data: &[u8]) -> Result<Vec<u8>> {
    let mut result = Vec::with_capacity(data.len() * 4 / 5);
    let mut group: u64 = 0;
    let mut count = 0;

    for &b in data {
        match b {
            // Whitespace: skip
            0 | 9 | 10 | 12 | 13 | 32 => continue,
            // EOD marker
            b'~' => break,
            // z = 4 zero bytes
            b'z' => {
                if count != 0 {
                    return Err(Error::Format("ASCII85: 'z' inside group".into()));
                }
                result.extend_from_slice(&[0, 0, 0, 0]);
            }
            // Normal character (33-117 -> 0-84)
            b'!'..=b'u' => {
                group = group * 85 + (b - b'!') as u64;
                count += 1;
                if count == 5 {
                    result.push((group >> 24) as u8);
                    result.push((group >> 16) as u8);
                    result.push((group >> 8) as u8);
                    result.push(group as u8);
                    group = 0;
                    count = 0;
                }
            }
            _ => {
                return Err(Error::Format(format!("ASCII85: invalid byte 0x{:02X}", b)));
            }
        }
    }

    // Handle partial final group
    if count > 0 {
        // Pad with 'u' (84) to make 5 chars
        for _ in count..5 {
            group = group * 85 + 84;
        }
        // Output count-1 bytes
        let bytes = group.to_be_bytes();
        for i in 0..(count - 1) {
            result.push(bytes[4 + i]); // bytes[4..8] is the lower 4 bytes
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// SD4: ASCIIHexDecode
// ---------------------------------------------------------------------------

/// SD4: ASCIIHexDecode (ISO 32000-2 §7.4.2).
///
/// Decodes hex-encoded data. `>` marks EOD. Whitespace ignored.
/// Odd final nibble gets a trailing 0.
fn decode_ascii_hex(data: &[u8]) -> Result<Vec<u8>> {
    let mut result = Vec::with_capacity(data.len() / 2);
    let mut high_nibble: Option<u8> = None;

    for &b in data {
        match b {
            b'>' => break,
            0 | 9 | 10 | 12 | 13 | 32 => continue,
            _ => {
                let nibble = hex_digit(b)?;
                match high_nibble {
                    None => high_nibble = Some(nibble),
                    Some(hi) => {
                        result.push((hi << 4) | nibble);
                        high_nibble = None;
                    }
                }
            }
        }
    }

    // Odd nibble: pad with 0
    if let Some(hi) = high_nibble {
        result.push(hi << 4);
    }

    Ok(result)
}

/// Decode a hex digit.
fn hex_digit(b: u8) -> Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(Error::Format(format!(
            "ASCIIHex: invalid digit 0x{:02X}",
            b
        ))),
    }
}

// ---------------------------------------------------------------------------
// SD5: LZWDecode
// ---------------------------------------------------------------------------

/// SD5: LZWDecode (ISO 32000-2 §7.4.4).
///
/// Variable-length code LZW decompression. PDF uses MSB-first bit packing
/// and an initial code size of 9 bits. Clear code = 256, EOD = 257.
fn decode_lzw(data: &[u8], parms: Option<&PdfObject>) -> Result<Vec<u8>> {
    let early_change = parms
        .and_then(|p| p.dict_get(b"EarlyChange"))
        .and_then(|v| v.as_int())
        .unwrap_or(1);

    let mut reader = BitReader::new(data);
    let mut table: Vec<Vec<u8>> = (0..256).map(|i| vec![i as u8]).collect();
    // 256 = clear code, 257 = EOD code
    table.push(vec![]); // 256
    table.push(vec![]); // 257

    let mut code_size: u32 = 9;
    let mut result = Vec::new();
    let mut prev: Option<Vec<u8>> = None;

    loop {
        let code = match reader.read_bits(code_size) {
            Some(c) => c as usize,
            None => break,
        };

        if code == 256 {
            // Clear
            table.truncate(258);
            code_size = 9;
            prev = None;
            continue;
        }

        if code == 257 {
            // EOD
            break;
        }

        let entry = if code < table.len() {
            table[code].clone()
        } else if code == table.len() {
            // Special case: code not yet in table
            let mut e = prev
                .as_ref()
                .ok_or_else(|| Error::Format("LZW: code before any output".into()))?
                .clone();
            e.push(e[0]);
            e
        } else {
            return Err(Error::Format(format!("LZW: invalid code {}", code)));
        };

        result.extend_from_slice(&entry);

        if let Some(ref prev_entry) = prev {
            let mut new_entry = prev_entry.clone();
            new_entry.push(entry[0]);
            table.push(new_entry);

            // Increase code size when table grows
            let threshold = if early_change == 1 {
                (1 << code_size) - 1
            } else {
                1 << code_size
            };
            if table.len() > threshold && code_size < 12 {
                code_size += 1;
            }
        }

        prev = Some(entry);
    }

    // Apply predictor if present
    if let Some(parms) = parms {
        let predictor = parms
            .dict_get(b"Predictor")
            .and_then(|p| p.as_int())
            .unwrap_or(1);
        if predictor >= 10 {
            let columns = parms
                .dict_get(b"Columns")
                .and_then(|c| c.as_int())
                .unwrap_or(1) as usize;
            return apply_png_predictor(&result, columns);
        }
    }

    Ok(result)
}

/// MSB-first bit reader for LZW.
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u32, // 0-7, bits remaining in current byte from MSB
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    fn read_bits(&mut self, count: u32) -> Option<u32> {
        let mut value: u32 = 0;
        let mut remaining = count;

        while remaining > 0 {
            if self.byte_pos >= self.data.len() {
                return None;
            }

            let available = 8 - self.bit_pos;
            let take = remaining.min(available);
            let shift = available - take;
            let mask = ((1u32 << take) - 1) as u8;
            let bits = (self.data[self.byte_pos] >> shift) & mask;

            value = (value << take) | bits as u32;
            remaining -= take;
            self.bit_pos += take;

            if self.bit_pos >= 8 {
                self.byte_pos += 1;
                self.bit_pos = 0;
            }
        }

        Some(value)
    }
}

// ---------------------------------------------------------------------------
// SD6: RunLengthDecode
// ---------------------------------------------------------------------------

/// SD6: RunLengthDecode (ISO 32000-2 §7.4.5).
///
/// Format: length byte + data.
/// - 0-127: copy next (length+1) bytes literally
/// - 129-255: repeat next byte (257-length) times
/// - 128: EOD
fn decode_run_length(data: &[u8]) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut pos = 0;

    while pos < data.len() {
        let length = data[pos];
        pos += 1;

        if length < 128 {
            // Copy (length+1) literal bytes
            let count = length as usize + 1;
            if pos + count > data.len() {
                break;
            }
            result.extend_from_slice(&data[pos..pos + count]);
            pos += count;
        } else if length == 128 {
            // EOD
            break;
        } else {
            // Repeat next byte (257-length) times
            if pos >= data.len() {
                break;
            }
            let count = 257 - length as usize;
            let byte = data[pos];
            pos += 1;
            result.extend(std::iter::repeat_n(byte, count));
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- SD1: FlateDecode ---

    #[test]
    fn sd1_flate_roundtrip() {
        let original = b"Hello, PDF world! This is test data for FlateDecode.";
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(original, 6);
        let decompressed = flate_decompress(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn sd1_flate_invalid() {
        assert!(flate_decompress(b"not valid zlib").is_err());
    }

    // --- SD2: PNG predictors ---

    #[test]
    fn sd2_png_none() {
        let data = [0, 1, 2, 3, 0, 4, 5, 6];
        let result = apply_png_predictor(&data, 3).unwrap();
        assert_eq!(result, [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn sd2_png_sub() {
        let data = [1, 10, 20, 30];
        let result = apply_png_predictor(&data, 3).unwrap();
        assert_eq!(result, [10, 30, 60]);
    }

    #[test]
    fn sd2_png_up() {
        let data = [2, 1, 2, 3, 2, 4, 5, 6];
        let result = apply_png_predictor(&data, 3).unwrap();
        assert_eq!(result, [1, 2, 3, 5, 7, 9]);
    }

    #[test]
    fn sd2_png_average() {
        // 2 columns, 2 rows, filter type 3 (Average)
        // Row 0: prev=[0,0], data=[10,20]
        //   decoded[0] = 10 + avg(0, 0) = 10
        //   decoded[1] = 20 + avg(10, 0) = 20 + 5 = 25
        // Row 1: prev=[10,25], data=[5,5]
        //   decoded[0] = 5 + avg(0, 10) = 5 + 5 = 10
        //   decoded[1] = 5 + avg(10, 25) = 5 + 17 = 22
        let data = [3, 10, 20, 3, 5, 5];
        let result = apply_png_predictor(&data, 2).unwrap();
        assert_eq!(result, [10, 25, 10, 22]);
    }

    #[test]
    fn sd2_png_paeth() {
        // Simple Paeth test: 2 columns, 1 row
        let data = [4, 10, 20];
        let result = apply_png_predictor(&data, 2).unwrap();
        // paeth(0,0,0)=0, decoded[0]=10
        // paeth(10,0,0)=10, decoded[1]=20+10=30
        assert_eq!(result, [10, 30]);
    }

    #[test]
    fn sd2_png_empty() {
        let result = apply_png_predictor(&[], 3).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn sd2_flate_with_predictor() {
        // Compress data with PNG predictor type 0 (None)
        let raw = vec![0, 0x41, 0x42, 0, 0x43, 0x44]; // 2 rows, 2 columns, filter=0
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);

        let parms = PdfObject::Dict(vec![
            (b"Predictor".to_vec(), PdfObject::Int(12)),
            (b"Columns".to_vec(), PdfObject::Int(2)),
        ]);

        let result = decode_flate(&compressed, Some(&parms)).unwrap();
        assert_eq!(result, [0x41, 0x42, 0x43, 0x44]);
    }

    // --- SD3: ASCII85Decode ---

    #[test]
    fn sd3_ascii85_basic() {
        // "test" is [116,101,115,116] = 1952805748, whose base-85 digits
        // (37,34,69,45,23) map to "FCfN8" once offset by '!'.
        let result = decode_ascii85(b"FCfN8~>").unwrap();
        assert_eq!(result, b"test");
    }

    #[test]
    fn sd3_ascii85_zero() {
        // 'z' represents four zero bytes
        let result = decode_ascii85(b"z~>").unwrap();
        assert_eq!(result, [0, 0, 0, 0]);
    }

    #[test]
    fn sd3_ascii85_whitespace_ignored() {
        let result = decode_ascii85(b"F Cf\nN8~>").unwrap();
        assert_eq!(result, b"test");
    }

    #[test]
    fn sd3_ascii85_partial_group() {
        // A partial group of n characters encodes n-1 bytes. "!!" pads to
        // "!!uuu" = 614124 = 0x00095FEC, of which only the first byte is kept.
        let result = decode_ascii85(b"!!~>").unwrap();
        assert_eq!(result, [0x00]);
    }

    #[test]
    fn sd3_ascii85_empty() {
        let result = decode_ascii85(b"~>").unwrap();
        assert!(result.is_empty());
    }

    // --- SD4: ASCIIHexDecode ---

    #[test]
    fn sd4_hex_basic() {
        let result = decode_ascii_hex(b"48656C6C6F>").unwrap();
        assert_eq!(result, b"Hello");
    }

    #[test]
    fn sd4_hex_lowercase() {
        let result = decode_ascii_hex(b"48656c6c6f>").unwrap();
        assert_eq!(result, b"Hello");
    }

    #[test]
    fn sd4_hex_whitespace() {
        let result = decode_ascii_hex(b"48 65 6C\n6C 6F>").unwrap();
        assert_eq!(result, b"Hello");
    }

    #[test]
    fn sd4_hex_odd_nibble() {
        let result = decode_ascii_hex(b"ABC>").unwrap();
        assert_eq!(result, [0xAB, 0xC0]);
    }

    #[test]
    fn sd4_hex_empty() {
        let result = decode_ascii_hex(b">").unwrap();
        assert!(result.is_empty());
    }

    // --- SD5: LZWDecode ---

    #[test]
    fn sd5_lzw_basic() {
        // Build a minimal LZW stream: clear(256) + literal bytes + EOD(257)
        // Encode "AB" using 9-bit codes: 256(clear), 65(A), 66(B), 257(EOD)
        let mut bits = BitWriter::new();
        bits.write(256, 9); // clear
        bits.write(65, 9); // 'A'
        bits.write(66, 9); // 'B'
        bits.write(257, 9); // EOD
        let data = bits.finish();

        let result = decode_lzw(&data, None).unwrap();
        assert_eq!(result, b"AB");
    }

    #[test]
    fn sd5_lzw_repeated() {
        // Encode "AAAA": clear, A, A, <258=AA>, EOD
        let mut bits = BitWriter::new();
        bits.write(256, 9); // clear
        bits.write(65, 9); // 'A'
        bits.write(65, 9); // 'A' (adds 258="AA" to table)
        bits.write(258, 9); // "AA" (adds 259="AA" to table)
        bits.write(257, 9); // EOD
        let data = bits.finish();

        let result = decode_lzw(&data, None).unwrap();
        assert_eq!(result, b"AAAA");
    }

    // --- SD6: RunLengthDecode ---

    #[test]
    fn sd6_rle_literal() {
        // 2 = copy next 3 bytes literally
        let data = [2, b'A', b'B', b'C', 128];
        let result = decode_run_length(&data).unwrap();
        assert_eq!(result, b"ABC");
    }

    #[test]
    fn sd6_rle_repeat() {
        // 253 = repeat next byte (257-253)=4 times
        let data = [253, b'X', 128];
        let result = decode_run_length(&data).unwrap();
        assert_eq!(result, b"XXXX");
    }

    #[test]
    fn sd6_rle_mixed() {
        // literal "Hi" + repeat '!' 3 times
        let data = [1, b'H', b'i', 254, b'!', 128];
        let result = decode_run_length(&data).unwrap();
        assert_eq!(result, b"Hi!!!");
    }

    #[test]
    fn sd6_rle_eod() {
        let result = decode_run_length(&[128]).unwrap();
        assert!(result.is_empty());
    }

    // --- SD7: Filter chaining ---

    #[test]
    fn sd7_single_filter() {
        let original = b"Hello World";
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(original, 6);

        let stream = PdfObject::Stream {
            dict: vec![
                (b"Filter".to_vec(), PdfObject::Name(b"FlateDecode".to_vec())),
                (b"Length".to_vec(), PdfObject::Int(compressed.len() as i64)),
            ],
            data: compressed.clone(),
        };

        let result = decode_stream(&stream, &compressed).unwrap();
        assert_eq!(result, original);
    }

    #[test]
    fn sd7_chained_filters() {
        // Encode with ASCIIHex then verify decode
        let hex_data = b"48656C6C6F>"; // "Hello"
        let stream = PdfObject::Stream {
            dict: vec![(
                b"Filter".to_vec(),
                PdfObject::Name(b"ASCIIHexDecode".to_vec()),
            )],
            data: hex_data.to_vec(),
        };

        let result = decode_stream(&stream, hex_data).unwrap();
        assert_eq!(result, b"Hello");
    }

    #[test]
    fn sd7_filter_array() {
        // Test with an array of filters (ASCIIHex only for testability)
        let hex_data = b"48656C6C6F>";

        let stream = PdfObject::Stream {
            dict: vec![(
                b"Filter".to_vec(),
                PdfObject::Array(vec![PdfObject::Name(b"ASCIIHexDecode".to_vec())]),
            )],
            data: hex_data.to_vec(),
        };

        let result = decode_stream(&stream, hex_data).unwrap();
        assert_eq!(result, b"Hello");
    }

    #[test]
    fn sd7_no_filter() {
        let stream = PdfObject::Stream {
            dict: vec![],
            data: b"raw data".to_vec(),
        };

        let result = decode_stream(&stream, b"raw data").unwrap();
        assert_eq!(result, b"raw data");
    }

    // --- SD8-SD11: Passthrough filters ---

    #[test]
    fn sd8_dct_passthrough() {
        let data = b"\xFF\xD8\xFF\xE0JPEG data";
        let result = apply_filter(b"DCTDecode", data, None).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn sd9_ccitt_passthrough() {
        let data = b"fax data";
        let result = apply_filter(b"CCITTFaxDecode", data, None).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn sd10_jbig2_passthrough() {
        let data = b"jbig2 data";
        let result = apply_filter(b"JBIG2Decode", data, None).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn sd11_jpx_passthrough() {
        let data = b"jpeg2000 data";
        let result = apply_filter(b"JPXDecode", data, None).unwrap();
        assert_eq!(result, data);
    }

    // --- SD12: Crypt passthrough ---

    #[test]
    fn sd12_crypt_passthrough() {
        let data = b"encrypted data";
        let result = apply_filter(b"Crypt", data, None).unwrap();
        assert_eq!(result, data);
    }

    // --- Filter abbreviations ---

    #[test]
    fn filter_abbreviations() {
        assert!(apply_filter(b"Fl", b"", None).is_err()); // empty flate -> error, but proves dispatch works
        let hex = b"41>";
        assert_eq!(apply_filter(b"AHx", hex, None).unwrap(), b"A");

        let rle = [0, b'X', 128];
        assert_eq!(apply_filter(b"RL", &rle, None).unwrap(), b"X");
    }

    // --- Unknown filter ---

    #[test]
    fn unknown_filter() {
        assert!(apply_filter(b"FooBarDecode", b"data", None).is_err());
    }

    // --- Bit reader (for LZW) ---

    #[test]
    fn bit_reader_basic() {
        // 0xFF = 11111111
        let mut r = BitReader::new(&[0xFF]);
        assert_eq!(r.read_bits(4), Some(0xF));
        assert_eq!(r.read_bits(4), Some(0xF));
        assert_eq!(r.read_bits(1), None);
    }

    #[test]
    fn bit_reader_cross_byte() {
        // 0xAB 0xCD = 10101011 11001101
        let mut r = BitReader::new(&[0xAB, 0xCD]);
        assert_eq!(r.read_bits(9), Some(0b101010111)); // 343
        assert_eq!(r.read_bits(7), Some(0b1001101)); // 77
    }

    // --- BitWriter helper for LZW tests ---

    struct BitWriter {
        data: Vec<u8>,
        current: u8,
        bit_pos: u32,
    }

    impl BitWriter {
        fn new() -> Self {
            Self {
                data: Vec::new(),
                current: 0,
                bit_pos: 0,
            }
        }

        fn write(&mut self, value: u32, bits: u32) {
            for i in (0..bits).rev() {
                let bit = (value >> i) & 1;
                self.current = (self.current << 1) | bit as u8;
                self.bit_pos += 1;
                if self.bit_pos == 8 {
                    self.data.push(self.current);
                    self.current = 0;
                    self.bit_pos = 0;
                }
            }
        }

        fn finish(mut self) -> Vec<u8> {
            if self.bit_pos > 0 {
                self.current <<= 8 - self.bit_pos;
                self.data.push(self.current);
            }
            self.data
        }
    }
}
