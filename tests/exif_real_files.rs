//! Integration tests: parse EXIF data from real JPEG files
//! and compare against ExifTool output.

use std::collections::HashMap;
use std::path::Path;

#[test]
fn parse_exif_from_jpegs() {
    let dir = Path::new("testdata/exiftool-images");
    if !dir.exists() {
        eprintln!("skipping: testdata not available");
        return;
    }

    let mut total = 0;
    let mut with_exif_ifd = 0;
    let mut with_gps = 0;
    let mut with_thumbnail = 0;
    let mut with_maker_note = 0;
    let mut failures = Vec::new();

    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext != "jpg" && ext != "jpeg" {
            continue;
        }

        let data = std::fs::read(&path).unwrap();
        let segs = match siftx::jpeg::parse_segments(&data) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for seg in &segs {
            if let Some(tiff_data) = seg.exif_tiff_data() {
                total += 1;
                match siftx::tiff::exif::ExifData::parse(tiff_data) {
                    Ok(exif) => {
                        // IFD0 should have entries
                        assert!(!exif.ifd0.entries.is_empty(), "empty IFD0 in {name}");

                        if exif.exif_ifd.is_some() {
                            with_exif_ifd += 1;
                        }
                        if exif.gps_ifd.is_some() {
                            with_gps += 1;
                        }
                        if let Some(thumb) = exif.thumbnail {
                            with_thumbnail += 1;
                            if thumb.len() >= 2 && thumb[0] == 0xFF && thumb[1] == 0xD8 {
                                // valid JPEG thumbnail
                            } else {
                                eprintln!(
                                    "  {name}: thumbnail not JPEG (first bytes: {:02X} {:02X})",
                                    thumb.get(0).copied().unwrap_or(0),
                                    thumb.get(1).copied().unwrap_or(0)
                                );
                            }
                        }
                        if exif.maker_note.is_some() {
                            with_maker_note += 1;
                        }
                    }
                    Err(e) => {
                        failures.push(format!("{name}: {e}"));
                    }
                }
            }
        }
    }

    eprintln!(
        "Parsed {total} EXIF blocks: {with_exif_ifd} ExifIFD, {with_gps} GPS, \
         {with_thumbnail} thumbnails, {with_maker_note} MakerNotes"
    );

    if !failures.is_empty() {
        panic!(
            "Failed to parse {} EXIF blocks:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    assert!(total > 0, "no EXIF data found in test corpus");
}

// ---------------------------------------------------------------------------
// ExifTool comparison test
// ---------------------------------------------------------------------------

/// Decode a "Raw profile type" text value (ImageMagick/GIMP format).
/// Format: "\n<type>\n<length>\n<hex data across lines>\n"
fn decode_raw_profile(text: &str) -> Option<Vec<u8>> {
    let mut lines = text.lines();
    lines.next()?; // skip empty first line (or type line)
    // Next non-empty line is the type (e.g., "iptc" or "exif")
    let type_line = lines.next()?.trim();
    if type_line.is_empty() {
        return None;
    }
    // Next line is the byte count
    let _count_line = lines.next()?.trim();
    // Remaining lines are hex data
    let mut hex = String::new();
    for line in lines {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            hex.push_str(trimmed);
        }
    }
    // Decode hex string to bytes
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let hex_bytes = hex.as_bytes();
    let mut i = 0;
    while i + 1 < hex_bytes.len() {
        let hi = hex_nibble(hex_bytes[i])?;
        let lo = hex_nibble(hex_bytes[i + 1])?;
        bytes.push((hi << 4) | lo);
        i += 2;
    }
    Some(bytes)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Extract IPTC data from raw Photoshop IRB (without "Photoshop 3.0\0" header).
fn extract_iptc_from_irb(data: &[u8]) -> Option<Vec<u8>> {
    let mut pos = 0;
    while pos + 12 <= data.len() {
        if &data[pos..pos + 4] != b"8BIM" {
            break;
        }
        pos += 4;
        if pos + 2 > data.len() {
            break;
        }
        let resource_id = u16::from_be_bytes([data[pos], data[pos + 1]]);
        pos += 2;
        if pos >= data.len() {
            break;
        }
        let name_len = data[pos] as usize;
        pos += 1;
        let padded_name_len = if (name_len + 1) % 2 == 0 {
            name_len
        } else {
            name_len + 1
        };
        pos += padded_name_len;
        if pos + 4 > data.len() {
            break;
        }
        let data_size =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if pos + data_size > data.len() {
            break;
        }
        if resource_id == 0x0404 {
            return Some(data[pos..pos + data_size].to_vec());
        }
        pos += data_size;
        if data_size % 2 == 1 {
            pos += 1;
        }
    }
    None
}

/// Extract tags from siftx into a HashMap<TagName, DisplayValue> for one file.
fn extract_sift_tags(path: &Path) -> Option<HashMap<String, String>> {
    use siftx::tiff::tags::TagGroup;

    let data = std::fs::read(path).ok()?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let mut tags_map = HashMap::new();

    // JPEG files
    if ext == "jpg" || ext == "jpeg" {
        let segs = siftx::jpeg::parse_segments(&data).ok()?;

        for seg in &segs {
            // EXIF tags
            if let Some(tiff_data) = seg.exif_tiff_data() {
                if let Ok(exif) = siftx::tiff::exif::ExifData::parse(tiff_data) {
                    let be = exif.header.big_endian;

                    // IFD0
                    extract_ifd_tags(&exif.ifd0, be, TagGroup::Ifd0, &mut tags_map);

                    // ExifIFD
                    if let Some(ref ifd) = exif.exif_ifd {
                        extract_ifd_tags(ifd, be, TagGroup::ExifIfd, &mut tags_map);
                    }

                    // GPS
                    if let Some(ref ifd) = exif.gps_ifd {
                        extract_ifd_tags(ifd, be, TagGroup::GpsIfd, &mut tags_map);
                    }

                    // Interop
                    if let Some(ref ifd) = exif.interop_ifd {
                        extract_ifd_tags(ifd, be, TagGroup::InteropIfd, &mut tags_map);
                    }

                    // IFD1 (thumbnail) - ExifTool reports these as [EXIF] too
                    if let Some(ref ifd) = exif.ifd1 {
                        extract_ifd_tags(ifd, be, TagGroup::Ifd1, &mut tags_map);
                    }

                    // MakerNotes
                    let tiff_base = tiff_data.as_ptr() as usize - data.as_ptr() as usize;
                    extract_maker_notes(&exif, tiff_data, be, tiff_base, &mut tags_map);
                }
            }

            // Canon CIFF tags (APP0 HEAPJPGM)
            if let Some(ciff_data) = seg.ciff_data() {
                let ciff_tags = siftx::tiff::maker_notes::decode_ciff(ciff_data);
                for tag in &ciff_tags {
                    tags_map.insert(format!("MakerNotes:{}", tag.name), tag.value.clone());
                }
            }

            // Qualcomm Camera Attributes (APP7)
            if let Some(qc_data) = seg.qualcomm_data() {
                let qc_tags = siftx::tiff::maker_notes::decode_qualcomm(qc_data);
                for tag in &qc_tags {
                    tags_map.insert(format!("MakerNotes:{}", tag.name), tag.value.clone());
                }
            }

            // XMP tags (standard)
            if let Some(xmp_bytes) = seg.xmp_data() {
                if let Some(xmp) = try_parse_xmp(xmp_bytes) {
                    insert_xmp_tags(&xmp, &mut tags_map);
                }
            }

            // IPTC tags
            if seg.is_photoshop() {
                if let Some(iptc_data) = siftx::iptc::extract_from_app13(seg.data) {
                    if let Ok(iptc) = siftx::iptc::parse_iptc(&iptc_data) {
                        // Group repeatable datasets by name, join with ", "
                        let mut iptc_groups: HashMap<String, Vec<String>> = HashMap::new();
                        for ds in &iptc.datasets {
                            let name = ds.name();
                            if name == "Unknown" {
                                continue;
                            }
                            if ds.record == 2 {
                                if name == "ApplicationRecordVersion" {
                                    // ExifTool shows this as a u16 decimal
                                    if ds.value.len() == 2 {
                                        let ver = u16::from_be_bytes([ds.value[0], ds.value[1]]);
                                        iptc_groups
                                            .entry(format!("IPTC:{name}"))
                                            .or_default()
                                            .push(ver.to_string());
                                    }
                                } else {
                                    let val = ds.as_string_lossy();
                                    let key = format!("IPTC:{name}");
                                    iptc_groups
                                        .entry(key)
                                        .or_default()
                                        .push(format_iptc_value(name, &val));
                                }
                            } else if ds.record == 1 && name == "CodedCharacterSet" {
                                // ExifTool shows ESC sequences as symbolic names
                                let val = if ds.value == b"\x1b\x25\x47" {
                                    "UTF8".to_string()
                                } else if ds.value == [0x1b, 0x2e, 0x41] {
                                    "UTF8".to_string() // ISO 2022 ESC . A
                                } else {
                                    format!("{:?}", ds.value)
                                };
                                iptc_groups
                                    .entry(format!("IPTC:{name}"))
                                    .or_default()
                                    .push(val);
                            }
                        }
                        for (key, values) in iptc_groups {
                            tags_map.insert(key, values.join(", "));
                        }
                    }
                }
            }
        }

        // Extended XMP: collect chunks sorted by offset, reassemble, and parse
        let ext_header = siftx::xmp::JPEG_XMP_EXT_HEADER;
        let mut ext_chunks: Vec<(u32, Vec<u8>)> = Vec::new(); // (offset, data)
        for seg in &segs {
            if seg.marker == siftx::jpeg::Marker::App1 && seg.data.starts_with(ext_header) {
                let payload = &seg.data[ext_header.len()..];
                if payload.len() >= 40 {
                    // 32 (digest) + 4 (total) + 4 (offset)
                    let _offset =
                        u32::from_be_bytes([payload[32], payload[33], payload[34], payload[35]]);
                    // Skip: payload[36..40] = offset
                    let off =
                        u32::from_be_bytes([payload[36], payload[37], payload[38], payload[39]]);
                    let chunk_data = payload[40..].to_vec();
                    ext_chunks.push((off, chunk_data));
                }
            }
        }
        if !ext_chunks.is_empty() {
            ext_chunks.sort_by_key(|(off, _)| *off);
            let mut assembled = Vec::new();
            for (_, data) in &ext_chunks {
                assembled.extend_from_slice(data);
            }
            if let Ok(xml) = String::from_utf8(assembled) {
                if let Some(xmp) = try_parse_xmp(xml.as_bytes()) {
                    insert_xmp_tags(&xmp, &mut tags_map);
                }
            }
        }
    }
    // TIFF files (including TIFF-based RAW: CR2, NEF, ARW, DNG, ORF, PEF, SRW, RW2)
    else if matches!(
        ext.as_str(),
        "tif"
            | "tiff"
            | "cr2"
            | "nef"
            | "nrw"
            | "arw"
            | "srf"
            | "sr2"
            | "dng"
            | "orf"
            | "pef"
            | "srw"
            | "rw2"
            | "rwl"
    ) {
        if let Ok(exif) = siftx::tiff::exif::ExifData::parse(&data) {
            let be = exif.header.big_endian;
            extract_ifd_tags(&exif.ifd0, be, TagGroup::Ifd0, &mut tags_map);
            if let Some(ref ifd) = exif.exif_ifd {
                extract_ifd_tags(ifd, be, TagGroup::ExifIfd, &mut tags_map);
            }
            if let Some(ref ifd) = exif.gps_ifd {
                extract_ifd_tags(ifd, be, TagGroup::GpsIfd, &mut tags_map);
            }

            // MakerNotes (TIFF - base offset is 0)
            extract_maker_notes(&exif, &data, be, 0, &mut tags_map);

            // IPTC data in TIFF: stored in IFD0 tag 0x83BB (33723)
            for entry in &exif.ifd0.entries {
                if entry.tag == 0x83BB {
                    if let Ok(iptc) = siftx::iptc::parse_iptc(entry.data) {
                        let mut iptc_groups: HashMap<String, Vec<String>> = HashMap::new();
                        for ds in &iptc.datasets {
                            let name = ds.name();
                            if name == "Unknown" {
                                continue;
                            }
                            if ds.record == 2 {
                                if name == "ApplicationRecordVersion" {
                                    if ds.value.len() == 2 {
                                        let ver = u16::from_be_bytes([ds.value[0], ds.value[1]]);
                                        iptc_groups
                                            .entry(format!("IPTC:{name}"))
                                            .or_default()
                                            .push(ver.to_string());
                                    }
                                } else {
                                    let val = ds.as_string_lossy();
                                    let key = format!("IPTC:{name}");
                                    iptc_groups
                                        .entry(key)
                                        .or_default()
                                        .push(format_iptc_value(name, &val));
                                }
                            } else if ds.record == 1 && name == "CodedCharacterSet" {
                                let val = if ds.value == b"\x1b\x25\x47"
                                    || ds.value == [0x1b, 0x2e, 0x41]
                                {
                                    "UTF8".to_string()
                                } else {
                                    format!("{:?}", ds.value)
                                };
                                iptc_groups
                                    .entry(format!("IPTC:{name}"))
                                    .or_default()
                                    .push(val);
                            }
                        }
                        for (key, values) in iptc_groups {
                            tags_map.insert(key, values.join(", "));
                        }
                    }
                }
            }

            // XMP data in TIFF: stored in IFD0 tag 0x02BC (700)
            for entry in &exif.ifd0.entries {
                if entry.tag == 0x02BC {
                    if let Some(xmp) = try_parse_xmp(entry.data) {
                        insert_xmp_tags(&xmp, &mut tags_map);
                    }
                }
            }
        }
    }
    // PNG files
    else if ext == "png" {
        if let Ok(chunks) = siftx::png::parse_chunks(&data) {
            if let Some(xmp_bytes) = siftx::png::find_xmp_data(&chunks) {
                if let Some(xmp) = try_parse_xmp(xmp_bytes) {
                    insert_xmp_tags(&xmp, &mut tags_map);
                }
            }
            // Try eXIf chunk first
            if let Some(tiff_data) = siftx::png::find_exif_chunk(&chunks) {
                if let Ok(exif) = siftx::tiff::exif::ExifData::parse(tiff_data) {
                    let be = exif.header.big_endian;
                    extract_ifd_tags(&exif.ifd0, be, TagGroup::Ifd0, &mut tags_map);
                    if let Some(ref ifd) = exif.exif_ifd {
                        extract_ifd_tags(ifd, be, TagGroup::ExifIfd, &mut tags_map);
                    }
                    if let Some(ref ifd) = exif.gps_ifd {
                        extract_ifd_tags(ifd, be, TagGroup::GpsIfd, &mut tags_map);
                    }
                    if let Some(ref ifd) = exif.interop_ifd {
                        extract_ifd_tags(ifd, be, TagGroup::InteropIfd, &mut tags_map);
                    }
                    if let Some(ref ifd) = exif.ifd1 {
                        extract_ifd_tags(ifd, be, TagGroup::Ifd1, &mut tags_map);
                    }
                    // MakerNotes (PNG - eXIf chunk, base offset 0)
                    extract_maker_notes(&exif, tiff_data, be, 0, &mut tags_map);
                }
            }
            // Also try "Raw profile type exif" in zTXt/tEXt chunks
            let texts = siftx::png::collect_text_chunks(&chunks);
            for tc in &texts {
                if tc.key == "Raw profile type exif" {
                    if let Some(raw) = decode_raw_profile(&tc.value) {
                        // Strip "Exif\0\0" header if present
                        let tiff_data = if raw.starts_with(b"Exif\0\0") {
                            &raw[6..]
                        } else {
                            &raw
                        };
                        if let Ok(exif) = siftx::tiff::exif::ExifData::parse(tiff_data) {
                            let be = exif.header.big_endian;
                            extract_ifd_tags(&exif.ifd0, be, TagGroup::Ifd0, &mut tags_map);
                            if let Some(ref ifd) = exif.exif_ifd {
                                extract_ifd_tags(ifd, be, TagGroup::ExifIfd, &mut tags_map);
                            }
                            if let Some(ref ifd) = exif.gps_ifd {
                                extract_ifd_tags(ifd, be, TagGroup::GpsIfd, &mut tags_map);
                            }
                            if let Some(ref ifd) = exif.interop_ifd {
                                extract_ifd_tags(ifd, be, TagGroup::InteropIfd, &mut tags_map);
                            }
                            if let Some(ref ifd) = exif.ifd1 {
                                extract_ifd_tags(ifd, be, TagGroup::Ifd1, &mut tags_map);
                            }
                            // MakerNotes (PNG raw profile - base 0)
                            extract_maker_notes(&exif, tiff_data, be, 0, &mut tags_map);
                        }
                    }
                }
                // "Raw profile type iptc" - contains Photoshop IRB with IPTC
                if tc.key == "Raw profile type iptc" {
                    if let Some(raw) = decode_raw_profile(&tc.value) {
                        // Raw profile may be bare IPTC or wrapped in Photoshop IRB (8BIM)
                        let iptc_bytes = if raw.starts_with(b"8BIM") {
                            // Parse IRB to extract IPTC resource 0x0404
                            extract_iptc_from_irb(&raw)
                        } else if raw.first() == Some(&0x1C) {
                            Some(raw.clone())
                        } else {
                            None
                        };
                        if let Some(iptc_data) = iptc_bytes {
                            if let Ok(iptc) = siftx::iptc::parse_iptc(&iptc_data) {
                                let mut iptc_groups: HashMap<String, Vec<String>> = HashMap::new();
                                for ds in &iptc.datasets {
                                    let name = ds.name();
                                    if name == "Unknown" {
                                        continue;
                                    }
                                    if ds.record == 2 {
                                        if name == "ApplicationRecordVersion" {
                                            if ds.value.len() == 2 {
                                                let ver =
                                                    u16::from_be_bytes([ds.value[0], ds.value[1]]);
                                                iptc_groups
                                                    .entry(format!("IPTC:{name}"))
                                                    .or_default()
                                                    .push(ver.to_string());
                                            }
                                        } else if let Some(val) = ds.as_str() {
                                            let key = format!("IPTC:{name}");
                                            iptc_groups
                                                .entry(key)
                                                .or_default()
                                                .push(format_iptc_value(name, val));
                                        }
                                    } else if ds.record == 1 && name == "CodedCharacterSet" {
                                        let val = if ds.value == b"\x1b\x25\x47"
                                            || ds.value == [0x1b, 0x2e, 0x41]
                                        {
                                            "UTF8".to_string()
                                        } else {
                                            format!("{:?}", ds.value)
                                        };
                                        iptc_groups
                                            .entry(format!("IPTC:{name}"))
                                            .or_default()
                                            .push(val);
                                    }
                                }
                                for (key, values) in iptc_groups {
                                    tags_map.insert(key, values.join(", "));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    // WebP files
    else if ext == "webp" {
        if let Ok(webp) = siftx::webp::parse_webp(&data) {
            if let Some(tiff_data) = siftx::webp::find_exif(&webp) {
                if let Ok(exif) = siftx::tiff::exif::ExifData::parse(tiff_data) {
                    let be = exif.header.big_endian;
                    extract_ifd_tags(&exif.ifd0, be, TagGroup::Ifd0, &mut tags_map);
                    if let Some(ref ifd) = exif.exif_ifd {
                        extract_ifd_tags(ifd, be, TagGroup::ExifIfd, &mut tags_map);
                    }
                    // MakerNotes (WebP - base 0)
                    extract_maker_notes(&exif, tiff_data, be, 0, &mut tags_map);
                }
            }
            if let Some(xmp_bytes) = siftx::webp::find_xmp(&webp) {
                if let Some(xmp) = try_parse_xmp(xmp_bytes) {
                    insert_xmp_tags(&xmp, &mut tags_map);
                }
            }
        }
    }
    // HEIC/HEIF files
    else if ext == "heic" || ext == "heif" {
        if let Ok(heif) = siftx::heif::parse_heif(&data) {
            if let Some(tiff_data) = heif.exif_data {
                if let Ok(exif) = siftx::tiff::exif::ExifData::parse(tiff_data) {
                    let be = exif.header.big_endian;
                    extract_ifd_tags(&exif.ifd0, be, TagGroup::Ifd0, &mut tags_map);
                    if let Some(ref ifd) = exif.exif_ifd {
                        extract_ifd_tags(ifd, be, TagGroup::ExifIfd, &mut tags_map);
                    }
                    if let Some(ref ifd) = exif.gps_ifd {
                        extract_ifd_tags(ifd, be, TagGroup::GpsIfd, &mut tags_map);
                    }
                    if let Some(ref ifd) = exif.interop_ifd {
                        extract_ifd_tags(ifd, be, TagGroup::InteropIfd, &mut tags_map);
                    }
                    // MakerNotes (HEIC - base 0)
                    extract_maker_notes(&exif, tiff_data, be, 0, &mut tags_map);
                }
            }
            if let Some(xmp_bytes) = heif.xmp_data {
                if let Some(xmp) = try_parse_xmp(xmp_bytes) {
                    insert_xmp_tags(&xmp, &mut tags_map);
                }
            }
        }
    }
    // GIF files
    else if ext == "gif" {
        if let Ok(gif) = siftx::gif::parse_gif(&data) {
            if let Some(xmp_bytes) = gif.xmp_data {
                if let Some(xmp) = try_parse_xmp(xmp_bytes) {
                    insert_xmp_tags(&xmp, &mut tags_map);
                }
            }
        }
    }
    // BMP files - no EXIF/XMP metadata, just structural info
    else if ext == "bmp" {
        // BMP has no standard EXIF/XMP/IPTC - nothing to extract
    }
    // Fujifilm RAF - extract embedded JPEG for EXIF/MakerNotes
    else if ext == "raf" {
        if data.len() > 0x5C && &data[..8] == b"FUJIFILM" {
            let jpeg_off =
                u32::from_be_bytes([data[0x54], data[0x55], data[0x56], data[0x57]]) as usize;
            let jpeg_len =
                u32::from_be_bytes([data[0x58], data[0x59], data[0x5A], data[0x5B]]) as usize;
            if jpeg_off + jpeg_len <= data.len() {
                let jpeg_data = &data[jpeg_off..jpeg_off + jpeg_len];
                if let Ok(segs) = siftx::jpeg::parse_segments(jpeg_data) {
                    for seg in &segs {
                        if let Some(tiff_data) = seg.exif_tiff_data() {
                            if let Ok(exif) = siftx::tiff::exif::ExifData::parse(tiff_data) {
                                let be = exif.header.big_endian;
                                extract_ifd_tags(&exif.ifd0, be, TagGroup::Ifd0, &mut tags_map);
                                if let Some(ref ifd) = exif.exif_ifd {
                                    extract_ifd_tags(ifd, be, TagGroup::ExifIfd, &mut tags_map);
                                }
                                if let Some(ref ifd) = exif.gps_ifd {
                                    extract_ifd_tags(ifd, be, TagGroup::GpsIfd, &mut tags_map);
                                }
                                if let Some(ref ifd) = exif.interop_ifd {
                                    extract_ifd_tags(ifd, be, TagGroup::InteropIfd, &mut tags_map);
                                }
                                if let Some(ref ifd1) = exif.ifd1 {
                                    extract_ifd_tags(ifd1, be, TagGroup::Ifd1, &mut tags_map);
                                }
                                extract_maker_notes(&exif, tiff_data, be, 0, &mut tags_map);
                            }
                        }
                        if let Some(xmp_bytes) = seg.xmp_data() {
                            if let Some(xmp) = try_parse_xmp(xmp_bytes) {
                                insert_xmp_tags(&xmp, &mut tags_map);
                            }
                        }
                    }
                }
            }
        }
    }
    // Canon CR3 - QuickTime/ISOBMFF container
    else if ext == "cr3" {
        // CR3 uses ISOBMFF with EXIF in 'moov/meta' boxes - needs QuickTime parser
        // For now, try parsing as HEIF since it shares the ftyp-based container
        if let Ok(heif) = siftx::heif::parse_heif(&data) {
            if let Some(tiff_data) = heif.exif_data {
                if let Ok(exif) = siftx::tiff::exif::ExifData::parse(tiff_data) {
                    let be = exif.header.big_endian;
                    extract_ifd_tags(&exif.ifd0, be, TagGroup::Ifd0, &mut tags_map);
                    if let Some(ref ifd) = exif.exif_ifd {
                        extract_ifd_tags(ifd, be, TagGroup::ExifIfd, &mut tags_map);
                    }
                    if let Some(ref ifd) = exif.gps_ifd {
                        extract_ifd_tags(ifd, be, TagGroup::GpsIfd, &mut tags_map);
                    }
                    extract_maker_notes(&exif, tiff_data, be, 0, &mut tags_map);
                }
            }
            if let Some(xmp_bytes) = heif.xmp_data {
                if let Some(xmp) = try_parse_xmp(xmp_bytes) {
                    insert_xmp_tags(&xmp, &mut tags_map);
                }
            }
        }
    }

    // Google HDR+ MakerNotes (protobuf in XMP GCamera:HdrPlusMakernote)
    if let Some(b64) = tags_map.get("XMP-GCamera:HdrPlusMakernote").cloned() {
        let hdrp_tags = siftx::tiff::maker_notes::decode_google_hdrp(&b64);
        for tag in &hdrp_tags {
            tags_map.insert(format!("MakerNotes:{}", tag.name), tag.value.clone());
        }
    }

    if tags_map.is_empty() {
        None
    } else {
        Some(tags_map)
    }
}

fn extract_maker_notes(
    exif: &siftx::tiff::exif::ExifData,
    tiff_data: &[u8],
    be: bool,
    tiff_base: usize,
    tags_map: &mut HashMap<String, String>,
) {
    use siftx::tiff::value::TagValue;

    if let Some(ref mnr) = exif.maker_note {
        let mut vendor = siftx::tiff::maker_notes::detect_vendor(mnr.data);
        if vendor == siftx::tiff::maker_notes::Vendor::Unknown {
            for entry in &exif.ifd0.entries {
                if entry.tag == 0x010F {
                    if let Some(val) = TagValue::from_entry(entry, be) {
                        vendor = siftx::tiff::maker_notes::vendor_from_make(&val.display());
                    }
                    break;
                }
            }
        }

        if let Some(mut mn) = siftx::tiff::maker_notes::parse_maker_note(mnr, tiff_data, be) {
            if mn.vendor == siftx::tiff::maker_notes::Vendor::Unknown {
                mn.vendor = vendor;
            }
            let mn_file_offset = tiff_base + mnr.offset;
            let decoded = siftx::tiff::maker_notes::decode_maker_tags_with_tiff(
                &mn,
                mnr.data,
                tiff_base,
                mn_file_offset,
                tiff_data,
            );
            for dt in &decoded {
                tags_map.insert(format!("MakerNotes:{}", dt.name), dt.value.clone());
            }
        }
    }
}

fn extract_ifd_tags(
    ifd: &siftx::tiff::Ifd,
    big_endian: bool,
    group: siftx::tiff::tags::TagGroup,
    tags_map: &mut HashMap<String, String>,
) {
    use siftx::tiff::tags::{self, TagGroup};
    use siftx::tiff::value::TagValue;

    let is_secondary = group == TagGroup::Ifd1;

    for entry in &ifd.entries {
        // Try the specified group first, then fall back to other groups
        // (some cameras put ExifIFD tags in IFD0)
        let tag_def = tags::find_tag(entry.tag, group).or_else(|| {
            if group == TagGroup::Ifd0 {
                tags::find_tag(entry.tag, TagGroup::ExifIfd)
                    .or_else(|| tags::find_tag(entry.tag, TagGroup::InteropIfd))
            } else {
                None
            }
        });
        if let Some(tag_def) = tag_def {
            // Skip pointer tags (they aren't real values)
            if tag_def.name == "ExifIFD"
                || tag_def.name == "GPSIFD"
                || tag_def.name == "InteropIFD"
                || tag_def.name == "MakerNote"
            {
                continue;
            }
            if let Some(val) = TagValue::from_entry(entry, big_endian) {
                let display = tags::print_value(tag_def, &val);
                // For IFD1, don't overwrite IFD0 values
                if is_secondary {
                    tags_map.entry(tag_def.name.to_string()).or_insert(display);
                } else {
                    tags_map.insert(tag_def.name.to_string(), display);
                }
            }
        }
    }
}

/// Insert XMP properties into tags_map with capitalized names and joined arrays.
fn insert_xmp_tags(xmp: &siftx::xmp::XmpData, tags_map: &mut HashMap<String, String>) {
    for prop in &xmp.properties {
        let ns_prefix = xmp_ns_prefix(&prop.namespace);
        // Capitalize first letter to match ExifTool convention
        let name = capitalize_first(&prop.name);
        let key = format!("XMP-{ns_prefix}:{name}");
        // Use all_strings() and join for array values
        let strings = prop.value.all_strings();
        if !strings.is_empty() {
            tags_map.entry(key).or_insert_with(|| strings.join(", "));
        }
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn try_parse_xmp(data: &[u8]) -> Option<siftx::xmp::XmpData> {
    // Try UTF-8 first
    if let Ok(xml) = std::str::from_utf8(data) {
        return std::panic::catch_unwind(|| siftx::xmp::parse_xmp(xml).ok())
            .ok()
            .flatten();
    }
    // Try UTF-16BE (null byte before each ASCII char, e.g., 0x00 0x3C for '<')
    if data.len() >= 4 && data[0] == 0x00 && data[1] == b'<' {
        let u16s: Vec<u16> = data
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        let xml = String::from_utf16_lossy(&u16s);
        return std::panic::catch_unwind(|| siftx::xmp::parse_xmp(&xml).ok())
            .ok()
            .flatten();
    }
    // Try UTF-16LE (null byte after each ASCII char)
    if data.len() >= 4 && data[0] == b'<' && data[1] == 0x00 {
        let u16s: Vec<u16> = data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let xml = String::from_utf16_lossy(&u16s);
        return std::panic::catch_unwind(|| siftx::xmp::parse_xmp(&xml).ok())
            .ok()
            .flatten();
    }
    None
}

fn xmp_ns_prefix(ns: &str) -> &'static str {
    match ns {
        "http://purl.org/dc/elements/1.1/" => "dc",
        "http://ns.adobe.com/xap/1.0/" => "xmp",
        "http://ns.adobe.com/exif/1.0/" => "exif",
        "http://ns.adobe.com/tiff/1.0/" => "tiff",
        "http://ns.adobe.com/photoshop/1.0/" => "photoshop",
        "http://ns.adobe.com/xap/1.0/mm/" => "xmpMM",
        "http://ns.adobe.com/xap/1.0/rights/" => "xmpRights",
        "adobe:ns:meta/" => "x",
        "http://ns.adobe.com/xap/1.0/sType/ResourceRef#" => "xmpMM",
        "http://ns.adobe.com/camera-raw-settings/1.0/" => "crs",
        "http://ns.adobe.com/xap/1.0/sType/ResourceEvent#" => "xmpMM",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#" => "rdf",
        "http://ns.adobe.com/exif/1.0/aux/" => "aux",
        "http://ns.camerabits.com/photomechanic/1.0/" => "photomechanic",
        "http://ns.microsoft.com/photo/1.0/" => "MicrosoftPhoto",
        "http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/" => "Iptc4xmpCore",
        "http://ns.google.com/photos/1.0/container/" => "Container",
        "http://ns.google.com/photos/1.0/container/item/" => "Container",
        "http://ns.adobe.com/hdr-gain-map/1.0/" => "HDRGainMap",
        "http://ns.google.com/photos/1.0/camera/" => "GCamera",
        _ => "other",
    }
}

/// How to invoke ExifTool for the comparison test.
///
/// `EXIFTOOL` may name either the `exiftool` executable or the Perl script; if
/// unset we fall back to whatever `exiftool` is on PATH. The comparison is a
/// convenience for validating tag output against the reference tool, so a
/// machine without it simply skips the test rather than failing.
fn exiftool_command() -> std::process::Command {
    let Ok(p) = std::env::var("EXIFTOOL") else {
        return std::process::Command::new("exiftool");
    };
    // A checkout of the ExifTool repo exposes the tool as a .pl script with no
    // execute bit, so that one case needs an explicit interpreter. Anything
    // else - a distro package, a wrapper, the shebanged script itself - is
    // executed directly, which is also the only form that works on Windows.
    if p.ends_with(".pl") {
        let mut c = std::process::Command::new("perl");
        c.arg(p);
        c
    } else {
        std::process::Command::new(p)
    }
}

/// Run ExifTool on a file and parse its output into HashMap<TagName, Value>.
/// Uses `-s -G` for short names with group prefixes.
fn run_exiftool(path: &Path) -> Option<HashMap<String, (String, String)>> {
    let output = exiftool_command()
        .arg("-s")
        .arg("-G")
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut tags = HashMap::new();
    for line in stdout.lines() {
        // Format: [Group]          TagName                : Value
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((group_and_tag, value)) = line.split_once(':') {
            let value = value.trim().to_string();
            let group_and_tag = group_and_tag.trim();
            // Split [Group] from TagName
            if let Some(rest) = group_and_tag.strip_prefix('[') {
                if let Some((group, tag_name)) = rest.split_once(']') {
                    let group = group.trim().to_string();
                    let tag_name = tag_name.trim().to_string();
                    tags.insert(tag_name, (group, value));
                }
            }
        }
    }
    Some(tags)
}

/// Normalize a value for fuzzy comparison.
/// Handles differences in formatting (trailing zeros, rational vs decimal, etc.)
fn normalize_value(s: &str) -> String {
    let s = s.trim();
    // Try to parse as a float and normalize
    if let Ok(f) = s.parse::<f64>() {
        // Remove trailing zeros: "14.0" -> "14", "2.8" -> "2.8"
        if f == f.floor() && f.abs() < 1e15 {
            return format!("{}", f as i64);
        }
        return format!("{:.6}", f)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string();
    }
    s.to_string()
}

#[test]
fn exiftool_comparison() {
    // Locate exiftool
    let exiftool_ok = exiftool_command()
        .arg("-ver")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !exiftool_ok {
        eprintln!("skipping: exiftool not available (set EXIFTOOL or put it on PATH)");
        return;
    }

    // Collect all test images
    let dirs = [
        "testdata/exiftool-images",
        "testdata/exif-samples",
        "testdata/fuzzing-seeds/heic",
        "testdata/fuzzing-seeds/jpg",
        "testdata/fuzzing-seeds/png",
        "testdata/fuzzing-seeds/tiff",
        "testdata/fuzzing-seeds/webp",
        "testdata/fuzzing-seeds/bmp",
    ];
    let extensions = [
        "jpg", "jpeg", "tif", "tiff", "png", "webp", "heic", "heif", "gif", "bmp", "cr2", "cr3",
        "nef", "nrw", "arw", "srf", "sr2", "dng", "orf", "pef", "srw", "rw2", "rwl", "raf",
    ];

    let mut all_files = Vec::new();
    for dir in &dirs {
        let path = Path::new(dir);
        if path.exists() {
            collect_image_files(path, &extensions, &mut all_files);
        }
    }
    all_files.sort();

    if all_files.is_empty() {
        eprintln!("skipping: no test images found");
        return;
    }

    // Counters
    let mut total_files = 0u32;
    let mut sift_parse_ok = 0u32;
    let mut exiftool_total_tags = 0u32;
    let mut sift_total_tags = 0u32;

    // Track: which ExifTool [EXIF] tags does siftx also extract?
    let mut exif_tags_total = 0u32; // ExifTool EXIF tags across all files
    let mut exif_tags_found = 0u32; // those also in siftx output
    let mut exif_tags_match = 0u32; // those with matching values

    // Same for XMP
    let mut xmp_tags_total = 0u32;
    let mut xmp_tags_found = 0u32;
    let mut xmp_tags_match = 0u32;

    // Same for IPTC
    let mut iptc_tags_total = 0u32;
    let mut iptc_tags_found = 0u32;
    let mut iptc_tags_match = 0u32;

    // Same for MakerNotes
    let mut mn_tags_total = 0u32;
    let mut mn_tags_found = 0u32;
    let mut mn_tags_match = 0u32;

    // Track missing tags (which ExifTool EXIF tags does siftx NOT have?)
    let mut missing_tags: HashMap<String, u32> = HashMap::new();
    // Track value mismatches
    let mut value_mismatches: Vec<String> = Vec::new();
    let max_mismatches = 80;

    for path in &all_files {
        let name = path
            .strip_prefix("testdata/")
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .to_string();

        total_files += 1;

        // Get ExifTool output
        let et_tags = match run_exiftool(path) {
            Some(t) => t,
            None => continue,
        };

        // Get siftx output
        let sift_tags = match extract_sift_tags(path) {
            Some(t) => t,
            None => continue,
        };

        sift_parse_ok += 1;
        sift_total_tags += sift_tags.len() as u32;

        // Compare: for each ExifTool tag, check if siftx also has it
        for (et_tag_name, (et_group, et_value)) in &et_tags {
            match et_group.as_str() {
                "EXIF" => {
                    // Skip pointer/structural tags
                    if et_tag_name == "ExifIFD"
                        || et_tag_name == "GPSIFD"
                        || et_tag_name == "InteropIFD"
                        || et_tag_name == "ThumbnailOffset"
                        || et_tag_name == "ThumbnailLength"
                        || et_tag_name == "Compression"
                        || et_tag_name == "ThumbnailImage"
                        || et_tag_name == "StripOffsets"
                        || et_tag_name == "StripByteCounts"
                        || et_tag_name == "TileOffsets"
                        || et_tag_name == "TileByteCounts"
                        || et_tag_name == "MakerNoteVersion"
                        || et_tag_name == "ColorMap"
                        || et_tag_name == "JPEGTables"
                        || et_tag_name == "Padding"
                        || et_tag_name == "DeviceSettingDescription"
                    {
                        continue;
                    }
                    // For RAW formats, skip IFD0 structural tags that differ between
                    // thumbnail (IFD0) and full-res (SubIFD) - ExifTool resolves to
                    // the full-res SubIFD, we read IFD0.
                    let is_raw = name.ends_with(".cr2")
                        || name.ends_with(".nef")
                        || name.ends_with(".nrw")
                        || name.ends_with(".arw")
                        || name.ends_with(".dng")
                        || name.ends_with(".orf")
                        || name.ends_with(".pef")
                        || name.ends_with(".rw2")
                        || name.ends_with(".raf")
                        || name.ends_with(".srw");
                    if is_raw
                        && matches!(
                            et_tag_name.as_str(),
                            "ImageWidth"
                                | "ImageHeight"
                                | "ImageLength"
                                | "BitsPerSample"
                                | "SamplesPerPixel"
                                | "PhotometricInterpretation"
                                | "RowsPerStrip"
                                | "SubfileType"
                                | "PlanarConfiguration"
                                | "CFARepeatPatternDim"
                                | "CFAPattern2"
                                | "PreviewImage"
                                | "PreviewImageStart"
                                | "PreviewImageLength"
                                | "JpgFromRaw"
                                | "JpgFromRawStart"
                                | "JpgFromRawLength"
                                | "OtherImage"
                                | "OtherImageStart"
                                | "OtherImageLength"
                                | "LinearizationTable"
                                | "BlackLevel"
                                | "WhiteLevel"
                                | "DefaultCropOrigin"
                                | "DefaultCropSize"
                                | "ActiveArea"
                                | "DNGVersion"
                                | "DNGBackwardVersion"
                                | "UniqueCameraModel"
                                | "RawDataUniqueID"
                                | "CalibrationIlluminant1"
                                | "CalibrationIlluminant2"
                                | "ColorMatrix1"
                                | "ColorMatrix2"
                                | "CameraCalibration1"
                                | "CameraCalibration2"
                                | "AnalogBalance"
                                | "AsShotNeutral"
                                | "BaselineExposure"
                                | "BaselineNoise"
                                | "BaselineSharpness"
                                | "BayerGreenSplit"
                                | "AntiAliasStrength"
                                | "ShadowScale"
                                | "SRawType"
                        )
                    {
                        continue;
                    }
                    // RW2 uses non-standard TIFF magic (II 55 00); IFD parsing
                    // may misalign certain structural tags
                    if name.ends_with(".rw2")
                        && matches!(
                            et_tag_name.as_str(),
                            "XResolution" | "YResolution" | "InteropIndex" | "InteropVersion"
                        )
                    {
                        continue;
                    }
                    exiftool_total_tags += 1;
                    exif_tags_total += 1;

                    // ExifTool uses "ModifyDate" for DateTime (0x0132)
                    // and "CreateDate" for DateTimeDigitized (0x9004)
                    // SiftX uses the EXIF spec names
                    let sift_name = exiftool_to_sift_name(et_tag_name);

                    if let Some(sift_value) = sift_tags.get(&sift_name) {
                        exif_tags_found += 1;
                        if values_match(et_tag_name, et_value, sift_value) {
                            exif_tags_match += 1;
                        } else if value_mismatches.len() < max_mismatches {
                            value_mismatches.push(format!(
                                "{name} [{et_group}] {et_tag_name}: exiftool={et_value:?} siftx={sift_value:?}"
                            ));
                        }
                    } else {
                        *missing_tags.entry(et_tag_name.clone()).or_insert(0) += 1;
                    }
                }
                "XMP" | "XMP-dc" | "XMP-xmp" | "XMP-exif" | "XMP-tiff" | "XMP-photoshop"
                | "XMP-xmpMM" | "XMP-xmpRights" | "XMP-crs" | "XMP-aux" | "XMP-photomechanic"
                | "XMP-microsoft" | "XMP-MicrosoftPhoto" | "XMP-iptcCore" | "XMP-Container"
                | "XMP-HDRGainMap" | "XMP-GCamera" => {
                    // Skip empty container tags and binary data
                    if et_value.is_empty() || et_value.starts_with("(Binary data") {
                        continue;
                    }
                    exiftool_total_tags += 1;
                    xmp_tags_total += 1;

                    // ExifTool with -G shows group like [XMP-tiff], siftx stores as XMP-tiff:name etc.
                    // Try the ExifTool group prefix first, then all others
                    let all_prefixes = [
                        "dc",
                        "xmp",
                        "tiff",
                        "exif",
                        "photoshop",
                        "xmpMM",
                        "xmpRights",
                        "x",
                        "crs",
                        "rdf",
                        "aux",
                        "photomechanic",
                        "MicrosoftPhoto",
                        "Iptc4xmpCore",
                        "Container",
                        "HDRGainMap",
                        "GCamera",
                        "other",
                    ];
                    let et_prefix = et_group.strip_prefix("XMP-").unwrap_or("");
                    let mut prefixes: Vec<&str> = Vec::with_capacity(all_prefixes.len());
                    if !et_prefix.is_empty() {
                        prefixes.push(et_prefix);
                    }
                    for p in &all_prefixes {
                        if *p != et_prefix {
                            prefixes.push(p);
                        }
                    }
                    // ExifTool renames some XMP tags
                    let alt_names: Vec<&str> = match et_tag_name.as_str() {
                        "ExifImageWidth" => vec!["PixelXDimension"],
                        "ExifImageHeight" => vec!["PixelYDimension"],
                        "ImageWidth" => vec!["ImageWidth", "PixelXDimension"],
                        "ImageHeight" => vec!["ImageLength", "PixelYDimension"],
                        // ExifTool maps MicrosoftPhoto:Rating to RatingPercent
                        "RatingPercent" => vec!["Rating"],
                        // ExifTool maps photoshop:ICCProfile to ICCProfileName
                        "ICCProfileName" => vec!["ICCProfile"],
                        // ExifTool maps crs:Temperature to ColorTemperature
                        "ColorTemperature" => vec!["Temperature"],
                        // ExifTool flattens TextLayers/LayerName -> TextLayerName
                        "TextLayerName" => vec!["TextLayersLayerName"],
                        "TextLayerText" => vec!["TextLayersLayerText"],
                        // ExifTool flattens DocumentAncestors/Originator -> DocumentAncestorsOriginator etc.
                        "JobRefName" => vec!["JobRefName", "JobName"],
                        // ExifTool renames ExposureBiasValue to ExposureCompensation
                        "ExposureCompensation" => vec!["ExposureBiasValue"],
                        // ExifTool flattens Iptc4xmpCore:CreatorContactInfo/CiEmailWork
                        "CreatorWorkEmail" => vec!["CiEmailWork", "CreatorContactInfoCiEmailWork"],
                        // ExifTool flattens Container:Directory items
                        "DirectoryItemMime" => vec!["DirectoryItemMime", "Mime"],
                        "DirectoryItemLength" => vec!["DirectoryItemLength", "Length"],
                        "DirectoryItemSemantic" => vec!["DirectoryItemSemantic", "Semantic"],
                        _ => vec![],
                    };
                    let mut found = false;
                    let names_to_try: Vec<&str> = std::iter::once(et_tag_name.as_str())
                        .chain(alt_names.into_iter())
                        .collect();
                    // Collect ALL matching siftx values across namespaces (some tags like
                    // NativeDigest exist in both tiff and exif namespaces)
                    let mut all_matches: Vec<(String, String)> = Vec::new();
                    for try_name in &names_to_try {
                        for pfx in &prefixes {
                            let sift_key = format!("XMP-{pfx}:{try_name}");
                            if let Some(sift_value) = sift_tags.get(&sift_key) {
                                all_matches.push((sift_key, sift_value.clone()));
                            }
                        }
                    }
                    if !all_matches.is_empty() {
                        xmp_tags_found += 1;
                        found = true;
                        // Accept if ANY matching namespace has the right value
                        let any_match = all_matches
                            .iter()
                            .any(|(_, v)| xmp_values_match(et_value, v));
                        if any_match {
                            xmp_tags_match += 1;
                        } else if value_mismatches.len() < max_mismatches {
                            value_mismatches.push(format!(
                                "{name} [XMP] {et_tag_name}: exiftool={et_value:?} siftx={:?}",
                                all_matches[0].1
                            ));
                        }
                    }
                    if !found {
                        *missing_tags
                            .entry(format!("XMP:{et_tag_name}"))
                            .or_insert(0) += 1;
                    }
                }
                "IPTC" => {
                    exiftool_total_tags += 1;
                    iptc_tags_total += 1;

                    let sift_key = format!("IPTC:{et_tag_name}");
                    if let Some(sift_value) = sift_tags.get(&sift_key) {
                        iptc_tags_found += 1;
                        if iptc_values_match(et_tag_name, et_value, sift_value) {
                            iptc_tags_match += 1;
                        } else if value_mismatches.len() < max_mismatches {
                            value_mismatches.push(format!(
                                "{name} [IPTC] {et_tag_name}: exiftool={et_value:?} siftx={sift_value:?}"
                            ));
                        }
                    } else {
                        *missing_tags
                            .entry(format!("IPTC:{et_tag_name}"))
                            .or_insert(0) += 1;
                    }
                }
                "MakerNotes" => {
                    // Skip binary data blobs (PreviewImage, etc.)
                    if et_value.starts_with("(Binary data") {
                        continue;
                    }
                    mn_tags_total += 1;
                    let sift_key = format!("MakerNotes:{et_tag_name}");
                    if let Some(sift_value) = sift_tags.get(&sift_key) {
                        mn_tags_found += 1;
                        if normalize_value(et_value) == normalize_value(sift_value) {
                            mn_tags_match += 1;
                        } else if value_mismatches.len() < max_mismatches {
                            value_mismatches.push(format!(
                                "{name} [MN] {et_tag_name}: exiftool={et_value:?} siftx={sift_value:?}"
                            ));
                        }
                    } else {
                        *missing_tags
                            .entry(format!("MakerNotes:{et_tag_name}"))
                            .or_insert(0) += 1;
                    }
                }
                // Skip File, ExifTool, Composite, ICC_Profile groups
                _ => {}
            }
        }
    }

    // Summary
    eprintln!("\n=== ExifTool comparison ===");
    eprintln!("Files scanned: {total_files}");
    eprintln!("SiftX parsed OK: {sift_parse_ok}");
    eprintln!("SiftX total tags extracted: {sift_total_tags}");
    eprintln!("ExifTool comparable tags: {exiftool_total_tags}");

    eprintln!("\nEXIF tags (IFD0 + ExifIFD + GPS + Interop):");
    eprintln!(
        "  Coverage: {exif_tags_found}/{exif_tags_total} ({:.1}%)",
        pct(exif_tags_found, exif_tags_total)
    );
    eprintln!(
        "  Value match: {exif_tags_match}/{exif_tags_found} ({:.1}%)",
        pct(exif_tags_match, exif_tags_found)
    );

    eprintln!("\nXMP tags:");
    eprintln!(
        "  Coverage: {xmp_tags_found}/{xmp_tags_total} ({:.1}%)",
        pct(xmp_tags_found, xmp_tags_total)
    );
    eprintln!(
        "  Value match: {xmp_tags_match}/{xmp_tags_found} ({:.1}%)",
        pct(xmp_tags_match, xmp_tags_found)
    );

    eprintln!("\nIPTC tags:");
    eprintln!(
        "  Coverage: {iptc_tags_found}/{iptc_tags_total} ({:.1}%)",
        pct(iptc_tags_found, iptc_tags_total)
    );
    eprintln!(
        "  Value match: {iptc_tags_match}/{iptc_tags_found} ({:.1}%)",
        pct(iptc_tags_match, iptc_tags_found)
    );

    eprintln!("\nMakerNotes tags:");
    eprintln!(
        "  Coverage: {mn_tags_found}/{mn_tags_total} ({:.1}%)",
        pct(mn_tags_found, mn_tags_total)
    );
    eprintln!(
        "  Value match: {mn_tags_match}/{mn_tags_found} ({:.1}%)",
        pct(mn_tags_match, mn_tags_found)
    );

    // Top missing tags
    let mut missing_sorted: Vec<_> = missing_tags.into_iter().collect();
    missing_sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let total_missing: u32 = missing_sorted.iter().map(|(_, c)| c).sum();
    eprintln!(
        "\nMost common missing tags (top 50, total {total_missing} across {} unique):",
        missing_sorted.len()
    );
    for (tag, count) in missing_sorted.iter().take(50) {
        eprintln!("  {count:3}x  {tag}");
    }

    if !value_mismatches.is_empty() {
        eprintln!("\nValue mismatches (first {}):", value_mismatches.len());
        for m in &value_mismatches {
            eprintln!("  {m}");
        }
    }
}

/// Map ExifTool tag names to siftx tag names where they differ.
fn exiftool_to_sift_name(et_name: &str) -> String {
    match et_name {
        // ExifTool renames some standard EXIF tags
        "ModifyDate" => "DateTime".to_string(),
        "CreateDate" => "DateTimeDigitized".to_string(),
        "CameraModelName" => "Model".to_string(),
        "SubSecModifyDate" => return "SubSecTime".to_string(),
        "SubSecDateTimeOriginal" => return "SubSecTimeOriginal".to_string(),
        "SubSecCreateDate" => return "SubSecTimeDigitized".to_string(),
        "OwnerName" => "CameraOwnerName".to_string(),
        _ => et_name.to_string(),
    }
}

/// Check if ExifTool and siftx values match, with format-aware comparison.
fn values_match(tag_name: &str, et_value: &str, sift_value: &str) -> bool {
    let et = et_value.trim();
    let sf = sift_value.trim();

    // Exact match
    if et == sf {
        return true;
    }

    // Normalized comparison
    if normalize_value(et) == normalize_value(sf) {
        return true;
    }

    // ExifTool shows rationals as decimals, siftx may show as fraction
    // e.g., ExifTool: "4.5" vs siftx: "9/2"
    if let Some(val) = try_parse_rational(sf) {
        if normalize_value(et) == normalize_value(&format!("{val}")) {
            return true;
        }
    }

    // ExifTool shows ExifVersion/FlashPixVersion as "0221", siftx may show raw bytes
    if tag_name == "ExifVersion" || tag_name == "FlashpixVersion" || tag_name == "FlashPixVersion" {
        // Both may output the same ASCII string, or siftx may have null padding
        let sf_trimmed = sf.trim_end_matches('\0');
        if et == sf_trimmed || et == sf {
            return true;
        }
        // ExifTool renders null bytes as ".", so ".." = two null bytes
        // SiftX renders them differently (e.g., "...." or raw)
        let et_dots = et.chars().all(|c| c == '.');
        let sf_dots = sf.chars().all(|c| c == '.');
        if et_dots || sf_dots {
            return true;
        } // both are all-null/garbage
        return false;
    }

    // YCbCrSubSampling: ExifTool shows "YCbCr4:2:0 (2 2)", siftx shows "2 2"
    if tag_name == "YCbCrSubSampling" {
        if et.contains(sf) {
            return true;
        }
    }

    // PhotometricInterpretation: ExifTool has more named values
    if tag_name == "PhotometricInterpretation" {
        if let Ok(n) = sf.parse::<u32>() {
            let name = match n {
                32845 => "Pixar LogLuv",
                32844 => "Pixar LogL",
                _ => "",
            };
            if !name.is_empty() && et == name {
                return true;
            }
        }
    }

    // ExifTool may show rational as decimal with units
    if let Some(val) = try_parse_rational(sf) {
        // ExifTool: "0.8" vs siftx: "1/1" - rational doesn't match
        // Only match if the fraction evaluates to the same number
        let et_no_units = et
            .trim_end_matches(" mm")
            .trim_end_matches(" m")
            .trim_end_matches(" s")
            .trim_end_matches(" EV");
        if normalize_value(et_no_units) == normalize_value(&format!("{val}")) {
            return true;
        }
    }

    // ExifTool shows GPS coordinates differently
    if tag_name.starts_with("GPS") {
        // GPS values have complex formatting, skip strict comparison
        return false;
    }

    // ExifTool shows ComponentsConfiguration as "Y, Cb, Cr, -"
    // SiftX might show raw bytes
    if tag_name == "ComponentsConfiguration" {
        return et == sf;
    }

    // UserComment: ExifTool may replace control chars with "."
    if tag_name == "UserComment" {
        let et_clean: String = et
            .chars()
            .map(|c| if c.is_control() { '.' } else { c })
            .collect();
        let sf_clean: String = sf
            .chars()
            .map(|c| if c.is_control() { '.' } else { c })
            .collect();
        if et_clean == sf_clean {
            return true;
        }
        // ExifTool trailing period = trailing control char
        if et.ends_with('.') && sf.ends_with(|c: char| c.is_control()) {
            let et_trim = &et[..et.len() - 1];
            let sf_trim = sf.trim_end_matches(|c: char| c.is_control());
            if et_trim == sf_trim {
                return true;
            }
        }
    }

    // ExifTool shows "Unknown (N)" for unknown enum values, siftx shows just "N"
    if et.starts_with("Unknown (") && et.ends_with(')') {
        let inner = &et[9..et.len() - 1];
        if inner == sf {
            return true;
        }
    }

    // ExifTool trailing period represents trailing newline
    if et.ends_with('.') && et[..et.len() - 1] == *sf {
        return true;
    }

    // Float arrays: compare as f64 values (ExifTool may round differently)
    if et.contains(' ') && sf.contains(' ') {
        let et_parts: Vec<&str> = et.split_whitespace().collect();
        let sf_parts: Vec<&str> = sf.split_whitespace().collect();
        if et_parts.len() == sf_parts.len() && et_parts.len() > 1 {
            let all_match = et_parts.iter().zip(sf_parts.iter()).all(|(e, s)| {
                if let (Ok(ev), Ok(sv)) = (e.parse::<f64>(), s.parse::<f64>()) {
                    (ev - sv).abs() < 1e-6 * ev.abs().max(1.0)
                } else {
                    e == s
                }
            });
            if all_match {
                return true;
            }
        }
    }

    false
}

/// Format IPTC values to match ExifTool output.
fn format_iptc_value(tag_name: &str, raw: &str) -> String {
    match tag_name {
        // Date: "20120622" -> "2012:06:22"
        "DateCreated"
        | "DigitalCreationDate"
        | "ReleaseDate"
        | "ExpirationDate"
        | "ReferenceDate"
            if raw.len() == 8 && raw.bytes().all(|b| b.is_ascii_digit()) =>
        {
            format!("{}:{}:{}", &raw[0..4], &raw[4..6], &raw[6..8])
        }
        // Time: "195231" -> "19:52:31", "021111+0100" -> "02:11:11+01:00"
        "TimeCreated" | "DigitalCreationTime" | "ReleaseTime" | "ExpirationTime"
            if raw.len() >= 6 =>
        {
            let time_part = format!("{}:{}:{}", &raw[0..2], &raw[2..4], &raw[4..6]);
            if raw.len() > 6 {
                // Timezone: "+0100" -> "+01:00" or "-0500" -> "-05:00"
                let tz = &raw[6..];
                if tz.len() >= 5 {
                    format!("{time_part}{}{}:{}", &tz[0..1], &tz[1..3], &tz[3..5])
                } else {
                    format!("{time_part}{tz}")
                }
            } else {
                time_part
            }
        }
        // Urgency: "8" -> "8 (least urgent)"
        "Urgency" => {
            let desc = match raw {
                "1" => " (most urgent)",
                "2" | "3" | "4" => " (high)",
                "5" => " (normal urgency)",
                "6" | "7" => " (low)",
                "8" => " (least urgent)",
                _ => "",
            };
            format!("{raw}{desc}")
        }
        _ => raw.to_string(),
    }
}

/// Compare IPTC values, accounting for formatting differences.
fn iptc_values_match(tag_name: &str, et_value: &str, sift_value: &str) -> bool {
    let et = et_value.trim();
    let sf = sift_value.trim();
    if et == sf {
        return true;
    }

    // Date formatting: ExifTool "2012:06:22" vs siftx "20120622"
    if tag_name.contains("Date") {
        let stripped: String = et.chars().filter(|c| c.is_ascii_digit()).collect();
        if stripped == sf {
            return true;
        }
    }

    // Time formatting: ExifTool "19:52:31" vs siftx "195231"
    if tag_name.contains("Time") {
        let stripped: String = et.chars().filter(|c| c.is_ascii_digit()).collect();
        if stripped == sf {
            return true;
        }
    }

    // Keywords: ExifTool joins with ", " but siftx may return only last entry
    // (IPTC repeatable datasets need proper aggregation)
    if tag_name == "Keywords" || tag_name == "SupplementalCategories" {
        if et.contains(sf) {
            return true;
        }
    }

    // ExifTool trailing period represents newline/control chars
    if et.ends_with('.') && et[..et.len() - 1] == *sf {
        return true;
    }

    // Control chars: ExifTool may replace with "." or strip
    let sf_cleaned: String = sf
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect();
    let et_cleaned: String = et
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect();
    if et_cleaned == sf_cleaned {
        return true;
    }

    false
}

/// Compare XMP values, accounting for formatting differences.
fn xmp_values_match(et_value: &str, sift_value: &str) -> bool {
    let et = et_value.trim();
    let sf = sift_value.trim();
    if et == sf {
        return true;
    }
    if normalize_value(et) == normalize_value(sf) {
        return true;
    }

    // ExifTool date format: "2021:04:11 15:47:53" or "2008:03:15 15:18:33-04:00"
    // vs ISO 8601: "2021-04-11T15:47:53" or "2008-03-15T15:18:33-04:00"
    // Convert siftx ISO to ExifTool format: replace date dashes with colons, T with space
    // Also handles comma-separated date lists (e.g., HistoryWhen)
    let convert_iso_date = |s: &str| -> String {
        if s.len() >= 10 && s.as_bytes().get(4) == Some(&b'-') && s.as_bytes().get(7) == Some(&b'-')
        {
            let date_part = s[..10].replace('-', ":");
            let rest = if s.len() > 10 { &s[10..] } else { "" };
            let rest = rest.strip_prefix('T').unwrap_or(rest);
            format!("{date_part} {rest}").trim().to_string()
        } else {
            s.to_string()
        }
    };
    if sf.contains(", ") {
        let converted: Vec<String> = sf.split(", ").map(|p| convert_iso_date(p.trim())).collect();
        if et == converted.join(", ") {
            return true;
        }
    } else {
        let sf_as_et = convert_iso_date(sf);
        if et == sf_as_et {
            return true;
        }
    }

    // Rational to decimal: "1200000/10000" == "120"
    if let Some(val) = try_parse_rational(sf) {
        if normalize_value(et) == normalize_value(&format!("{val}")) {
            return true;
        }
        // ExifTool may add units: "75.0 mm" vs "75/1"
        let et_no_units = et.trim_end_matches(" mm").trim_end_matches(" m");
        if normalize_value(et_no_units) == normalize_value(&format!("{val}")) {
            return true;
        }
    }

    // ExifTool applies PrintConv to XMP tiff/exif values
    if let Ok(n) = sf.parse::<u32>() {
        let converted = match (et, n) {
            ("Horizontal (normal)", 1) => true,
            ("Mirror horizontal", 2) => true,
            ("Rotate 180", 3) => true,
            ("Mirror vertical", 4) => true,
            ("Mirror horizontal and rotate 270 CW", 5) => true,
            ("Rotate 90 CW", 6) => true,
            ("Mirror horizontal and rotate 90 CW", 7) => true,
            ("Rotate 270 CW", 8) => true,
            ("sRGB", 1) if et == "sRGB" => true,
            ("Uncalibrated", 65535) => true,
            ("inches", 2) | ("cm", 3) => true,
            ("Chunky", 1) if et == "Chunky" => true,
            ("Planar", 2) if et == "Planar" => true,
            ("Centered", 1) if et == "Centered" => true,
            ("Co-sited", 2) if et == "Co-sited" => true,
            ("RGB", 2) if et == "RGB" => true, // PhotometricInterpretation
            ("RGB", 3) if et == "RGB" => true, // ColorMode
            // Covers LightSource, FlashMode, MeteringMode and
            // SubjectDistanceRange, which all spell value 0 "Unknown".
            ("JPEG (old-style)", 6) => true,
            // Flash sub-fields
            ("No return detection", 0) => true, // FlashReturn
            ("Fired", 1) => true,               // FlashFired
            ("Auto", 3) => true,                // FlashMode
            ("Off", 2) => true,                 // FlashMode
            ("On", 1) => true,                  // FlashMode
            ("No Flash", 0) => true,            // FlashFunction
            // WhiteBalance
            ("Auto", 0) if et == "Auto" => true,
            ("Manual", 1) if et == "Manual" => true,
            // SceneCaptureType
            ("Standard", 0) if et == "Standard" => true,
            ("Landscape", 1) if et == "Landscape" => true,
            ("Portrait", 2) if et == "Portrait" => true,
            ("Night", 3) if et == "Night" => true,
            // ExposureMode
            ("Auto", 0) => true,
            ("Manual", 1) => true,
            ("Auto bracket", 2) => true,
            // CustomRendered
            ("Normal", 0) if et == "Normal" => true,
            ("Custom", 1) if et == "Custom" => true,
            // MeteringMode
            ("Average", 1) if et == "Average" => true,
            ("Center-weighted average", 2) => true,
            ("Spot", 3) if et == "Spot" => true,
            ("Multi-spot", 4) => true,
            ("Multi-segment", 5) => true,
            ("Partial", 6) if et == "Partial" => true,
            // Contrast/Saturation/Sharpness
            ("Normal", 0) => true,
            ("Low", 1) if et == "Low" => true,
            ("High", 2) if et == "High" => true,
            ("Soft", 1) if et == "Soft" => true,
            ("Hard", 2) if et == "Hard" => true,
            // GainControl
            ("None", 0) if et == "None" => true,
            ("Low gain up", 1) => true,
            ("High gain up", 2) => true,
            ("Low gain down", 3) => true,
            ("High gain down", 4) => true,
            // SubjectDistanceRange
            ("Unknown", 0) => true,
            ("Macro", 1) if et == "Macro" => true,
            ("Close", 2) if et == "Close" => true,
            ("Distant", 3) if et == "Distant" => true,
            // ExposureProgram
            ("Not Defined", 0) => true,
            ("Program AE", 2) => true,
            ("Aperture-priority AE", 3) => true,
            ("Shutter speed priority AE", 4) => true,
            // SensingMethod
            ("Not defined", 1) => true,
            ("One-chip color area", 2) => true,
            ("Two-chip color area", 3) => true,
            ("Three-chip color area", 4) => true,
            ("Color sequential area", 5) => true,
            ("Trilinear", 7) => true,
            ("Color sequential linear", 8) => true,
            // PerspectiveUpright / other "Off" = 0
            ("Off", 0) if et == "Off" => true,
            _ => false,
        };
        if converted {
            return true;
        }
    }

    // ComponentsConfiguration: "Y, Cb, Cr, -" vs "1, 2, 3, 0"
    if et == "Y, Cb, Cr, -" && sf == "1, 2, 3, 0" {
        return true;
    }
    if et == "Y, Cb, Cr" && sf == "1, 2, 3" {
        return true;
    }

    // ApproximateFocusDistance: "infinity" vs "4294967295/1"
    if et == "infinity" && (sf == "4294967295/1" || sf == "inf") {
        return true;
    }

    // Urgency: ExifTool adds description - "8 (least urgent)", "1 (most urgent)", etc.
    if et.starts_with(sf) && et.contains("(") {
        // e.g., "8 (least urgent)" matches "8"
        if et.starts_with(&format!("{sf} (")) {
            return true;
        }
    }

    // Prefs: ExifTool formats "0:0:0:-00001" as "Tagged:0, ColorClass:0, Rating:0, FrameNum:-00001"
    if et.starts_with("Tagged:") && sf.contains(':') {
        let parts: Vec<&str> = sf.split(':').collect();
        if parts.len() == 4 {
            let expected = format!(
                "Tagged:{}, ColorClass:{}, Rating:{}, FrameNum:{}",
                parts[0], parts[1], parts[2], parts[3]
            );
            if et == expected {
                return true;
            }
        }
    }

    // HDRGainMapVersion: ExifTool converts integer to dotted version (65536 -> "0.1.0.0")
    if let Ok(n) = sf.parse::<u32>() {
        let version = format!(
            "{}.{}.{}.{}",
            (n >> 24) & 0xFF,
            (n >> 16) & 0xFF,
            (n >> 8) & 0xFF,
            n & 0xFF
        );
        if et == version {
            return true;
        }
    }

    // Multiline XMP: siftx may include/exclude trailing whitespace/newlines differently
    if normalize_value(&et.replace('\n', " ")) == normalize_value(&sf.replace('\n', " ")) {
        return true;
    }

    // ExifTool -s -G represents trailing newlines as "." in the output
    // e.g., "Enduring Freedom." when the actual value is "Enduring Freedom\n"
    if et.ends_with('.') && et[..et.len() - 1] == *sf {
        return true;
    }

    false
}

fn try_parse_rational(s: &str) -> Option<f64> {
    let (n, d) = s.split_once('/')?;
    let n: f64 = n.trim().parse().ok()?;
    let d: f64 = d.trim().parse().ok()?;
    if d == 0.0 {
        return None;
    }
    Some(n / d)
}

fn pct(num: u32, denom: u32) -> f64 {
    if denom == 0 {
        0.0
    } else {
        num as f64 * 100.0 / denom as f64
    }
}

fn collect_image_files(dir: &Path, extensions: &[&str], files: &mut Vec<std::path::PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().map(|n| n == ".git").unwrap_or(false) {
                continue;
            }
            collect_image_files(&path, extensions, files);
        } else {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if extensions.contains(&ext.as_str()) {
                files.push(path);
            }
        }
    }
}
