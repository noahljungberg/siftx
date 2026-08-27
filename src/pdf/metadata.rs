//! PDF metadata extraction (PM1-PM11).
//!
//! Extracts document-level metadata: Info dict, XMP, version, page info,
//! PDF subtype detection, encryption, tagged PDF, JavaScript detection.
//! Per ISO 32000-2 §14.3 and related sections.

use super::decode;
use super::document::Document;
use super::object::PdfObject;
use crate::core::Result;

/// Complete PDF metadata.
#[derive(Debug, Clone)]
pub struct PdfMetadata {
    // PM1: Info dictionary fields
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date: Option<String>,
    pub mod_date: Option<String>,
    pub trapped: Option<String>,

    // PM2: Raw XMP metadata (XML bytes)
    pub xmp: Option<Vec<u8>>,

    // PM3: PDF version
    pub header_version: Option<String>,
    pub catalog_version: Option<String>,
    /// Effective version: higher of header vs catalog.
    pub version: Option<String>,

    // PM4: Page count
    pub page_count: u32,

    // PM5 + PM6: Per-page info
    pub pages: Vec<PageInfo>,

    // PM7: PDF subtype
    pub subtype: Option<PdfSubtype>,

    // PM8: Encryption
    pub encryption: Option<EncryptionInfo>,

    // PM9: Tagged PDF
    pub is_tagged: bool,

    // PM10: JavaScript
    pub has_javascript: bool,

    // PM11: Linearization
    pub is_linearized: bool,
}

/// PM5 + PM6: Per-page metadata.
#[derive(Debug, Clone)]
pub struct PageInfo {
    pub media_box: [f64; 4],
    pub crop_box: [f64; 4],
    pub bleed_box: Option<[f64; 4]>,
    pub trim_box: Option<[f64; 4]>,
    pub art_box: Option<[f64; 4]>,
    pub rotate: i64,
}

/// PM7: PDF subtype/conformance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfSubtype {
    PdfA { part: u32, conformance: String },
    PdfX(String),
    PdfE(String),
    PdfUA { part: u32 },
    PdfVT(String),
}

/// PM8: Encryption details.
#[derive(Debug, Clone)]
pub struct EncryptionInfo {
    pub algorithm: String,
    pub key_length: i64,
    pub permissions: i64,
    pub revision: i64,
}

impl<'a> Document<'a> {
    /// Extract all metadata from the PDF.
    pub fn metadata(&self) -> Result<PdfMetadata> {
        let info = self.extract_info()?;
        let xmp = self.extract_xmp()?;
        let (header_version, catalog_version) = self.extract_versions()?;
        let version = effective_version(&header_version, &catalog_version);
        let page_count = self.page_count().unwrap_or(0);
        let pages = self.extract_page_info()?;
        let subtype = self.detect_subtype(&xmp);
        let encryption = self.extract_encryption()?;
        let is_tagged = self.is_tagged()?;
        let has_javascript = self.has_javascript()?;
        let is_linearized = self.is_linearized();

        Ok(PdfMetadata {
            title: info.0,
            author: info.1,
            subject: info.2,
            keywords: info.3,
            creator: info.4,
            producer: info.5,
            creation_date: info.6,
            mod_date: info.7,
            trapped: info.8,
            xmp,
            header_version,
            catalog_version,
            version,
            page_count,
            pages,
            subtype,
            encryption,
            is_tagged,
            has_javascript,
            is_linearized,
        })
    }

    // --- PM1: Info dictionary ---

    #[allow(clippy::type_complexity)]
    fn extract_info(
        &self,
    ) -> Result<(
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> {
        let info = match self.info()? {
            Some(info) => info,
            None => return Ok((None, None, None, None, None, None, None, None, None)),
        };

        let resolve_string = |key: &[u8]| -> Option<String> {
            let val = info.dict_get(key)?;
            // Resolve indirect references
            let resolved = match val {
                PdfObject::Ref(_) => self.resolve_obj(val).ok()?,
                other => other.clone(),
            };
            match &resolved {
                PdfObject::String(bytes) => {
                    let s = decode_pdf_text(bytes);
                    let trimmed = s.trim().to_string();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    }
                }
                _ => None,
            }
        };

        Ok((
            resolve_string(b"Title"),
            resolve_string(b"Author"),
            resolve_string(b"Subject"),
            resolve_string(b"Keywords"),
            resolve_string(b"Creator"),
            resolve_string(b"Producer"),
            resolve_string(b"CreationDate"),
            resolve_string(b"ModDate"),
            info.dict_get(b"Trapped")
                .and_then(|t| t.as_name_str())
                .map(|s| s.to_string()),
        ))
    }

    // --- PM2: XMP metadata stream ---

    fn extract_xmp(&self) -> Result<Option<Vec<u8>>> {
        let catalog = self.catalog()?;

        let metadata_ref = match catalog.dict_get(b"Metadata") {
            Some(r) => r,
            None => return Ok(None),
        };

        let metadata_obj = self.resolve_obj(metadata_ref)?;

        let raw = match metadata_obj.stream_data() {
            Some(data) => data,
            None => return Ok(None),
        };

        // XMP metadata streams may be compressed
        let decoded = decode::decode_stream(&metadata_obj, raw).unwrap_or_else(|_| raw.to_vec());

        Ok(Some(decoded))
    }

    // --- PM3: PDF version ---

    fn extract_versions(&self) -> Result<(Option<String>, Option<String>)> {
        // Header version from %PDF-x.y - scan up to 1024 bytes for the marker.
        // Parse major.minor as integers to match poppler behavior:
        //   %PDF-1.70 -> "1.70", %PDF-2.01 -> "2.1", %PDF-14 -> "14.0"
        //   Invalid/missing -> "0.0"
        let search_len = self.data.len().min(1024);
        let header = self.data[..search_len]
            .windows(5)
            .position(|w| w == b"%PDF-")
            .map(|pos| {
                let start = pos + 5;
                parse_version_string(&self.data[start..])
            })
            .unwrap_or_else(|| Some("0.0".into()));

        // Catalog /Version - only accept valid N.N format
        let catalog_ver = self
            .catalog()
            .ok()
            .and_then(|c| c.dict_get(b"Version")?.as_name_str().map(|s| s.to_string()))
            .filter(|v| is_valid_catalog_version(v));

        Ok((header, catalog_ver))
    }

    // --- PM5 + PM6: Per-page info ---

    fn extract_page_info(&self) -> Result<Vec<PageInfo>> {
        let pages = self.pages()?;
        Ok(pages
            .iter()
            .map(|p| PageInfo {
                media_box: p.media_box,
                crop_box: p.crop_box,
                bleed_box: parse_rect(p.dict.dict_get(b"BleedBox")),
                trim_box: parse_rect(p.dict.dict_get(b"TrimBox")),
                art_box: parse_rect(p.dict.dict_get(b"ArtBox")),
                rotate: p.rotate,
            })
            .collect())
    }

    // --- PM7: PDF subtype detection ---

    fn detect_subtype(&self, xmp: &Option<Vec<u8>>) -> Option<PdfSubtype> {
        let xmp_data = xmp.as_ref()?;
        let xmp_str = std::str::from_utf8(xmp_data).ok()?;

        // PDF/A: look for pdfaid:part and pdfaid:conformance
        if let Some(part) = extract_xmp_value(xmp_str, "pdfaid:part") {
            let conformance =
                extract_xmp_value(xmp_str, "pdfaid:conformance").unwrap_or_else(|| "B".to_string());
            return Some(PdfSubtype::PdfA {
                part: part.parse().unwrap_or(1),
                conformance,
            });
        }

        // PDF/X: pdfxid:GTS_PDFXVersion or pdfx:GTS_PDFXVersion
        if let Some(version) = extract_xmp_value(xmp_str, "pdfxid:GTS_PDFXVersion")
            .or_else(|| extract_xmp_value(xmp_str, "pdfx:GTS_PDFXVersion"))
        {
            return Some(PdfSubtype::PdfX(version));
        }

        // PDF/UA: pdfuaid:part
        if let Some(part) = extract_xmp_value(xmp_str, "pdfuaid:part") {
            return Some(PdfSubtype::PdfUA {
                part: part.parse().unwrap_or(1),
            });
        }

        // PDF/E: pdfe:ISO_PDFEVersion
        if let Some(version) = extract_xmp_value(xmp_str, "pdfe:ISO_PDFEVersion") {
            return Some(PdfSubtype::PdfE(version));
        }

        // PDF/VT: pdfvtid:GTS_PDFVTVersion
        if let Some(version) = extract_xmp_value(xmp_str, "pdfvtid:GTS_PDFVTVersion") {
            return Some(PdfSubtype::PdfVT(version));
        }

        None
    }

    // --- PM8: Encryption ---

    fn extract_encryption(&self) -> Result<Option<EncryptionInfo>> {
        let encrypt = match self.xref.trailer.dict_get(b"Encrypt") {
            Some(e) => self.resolve_obj(e)?,
            None => return Ok(None),
        };

        // Only report encryption if the security handler authenticated successfully.
        // A stale /Encrypt dict (e.g. from a reconstructed PDF) won't authenticate.
        if !self.is_encrypted() || !self.is_authenticated() {
            return Ok(None);
        }

        let filter = pdf_string(&encrypt, b"Filter").unwrap_or_default();
        let v = encrypt.dict_get(b"V").and_then(|v| v.as_int()).unwrap_or(0);
        let length = encrypt
            .dict_get(b"Length")
            .and_then(|l| l.as_int())
            .unwrap_or(40);
        let r = encrypt.dict_get(b"R").and_then(|r| r.as_int()).unwrap_or(0);
        let p = encrypt.dict_get(b"P").and_then(|p| p.as_int()).unwrap_or(0);

        let algorithm = match v {
            0 => "Undocumented".to_string(),
            1 => format!("{filter} V1 (RC4 40-bit)"),
            2 => format!("{filter} V2 (RC4 {length}-bit)"),
            3 => format!("{filter} V3 (unpublished)"),
            4 => format!("{filter} V4 (AES-128 or RC4)"),
            5 => format!("{filter} V5 (AES-256)"),
            _ => format!("{filter} V{v}"),
        };

        Ok(Some(EncryptionInfo {
            algorithm,
            key_length: length,
            permissions: p,
            revision: r,
        }))
    }

    // --- PM9: Tagged PDF ---

    fn is_tagged(&self) -> Result<bool> {
        let catalog = self.catalog()?;

        // Poppler checks only /MarkInfo << /Marked true >>, not /StructTreeRoot.
        if let Some(mark_info) = catalog.dict_get(b"MarkInfo") {
            let resolved = self.resolve_obj(mark_info)?;
            if resolved
                .dict_get(b"Marked")
                .and_then(|m| self.resolve_obj(m).ok())
                .and_then(|m| m.as_bool())
                == Some(true)
            {
                return Ok(true);
            }
        }

        Ok(false)
    }

    // --- PM10: JavaScript detection ---

    fn has_javascript(&self) -> Result<bool> {
        let catalog = self.catalog()?;

        // Check /Names -> /JavaScript
        if let Some(names) = catalog.dict_get(b"Names") {
            let names_dict = self.resolve_obj(names)?;
            if names_dict.dict_get(b"JavaScript").is_some() {
                return Ok(true);
            }
        }

        // Check /OpenAction for JavaScript
        if let Some(open_action) = catalog.dict_get(b"OpenAction") {
            let action = self.resolve_obj(open_action)?;
            if action.dict_get(b"S").and_then(|s| s.as_name()) == Some(b"JavaScript") {
                return Ok(true);
            }
        }

        // Check /AA (additional actions) on catalog
        if let Some(aa) = catalog.dict_get(b"AA") {
            if self.actions_contain_js(aa)? {
                return Ok(true);
            }
        }

        // Check /AcroForm -> /Fields for JavaScript in form widget actions
        if let Some(acro) = catalog.dict_get(b"AcroForm") {
            let acro_dict = self.resolve_obj(acro)?;
            if self.form_fields_contain_js(&acro_dict)? {
                return Ok(true);
            }
        }

        // Check pages for JavaScript actions (page /AA and annotations)
        if let Ok(pages) = self.pages() {
            for page in &pages {
                // Check /AA (additional actions) on the page itself
                if let Some(aa) = page.dict.dict_get(b"AA") {
                    if self.actions_contain_js(aa)? {
                        return Ok(true);
                    }
                }
                if let Some(annots_obj) = page.dict.dict_get(b"Annots") {
                    let annots = self.resolve_obj(annots_obj)?;
                    if let Some(arr) = annots.as_array() {
                        for annot_ref in arr {
                            let annot = self.resolve_obj(annot_ref)?;
                            // Check /A (action)
                            if let Some(a) = annot.dict_get(b"A") {
                                let action = self.resolve_obj(a)?;
                                if action.dict_get(b"S").and_then(|s| s.as_name())
                                    == Some(b"JavaScript")
                                {
                                    return Ok(true);
                                }
                            }
                            // Check /AA (additional actions)
                            if let Some(aa) = annot.dict_get(b"AA") {
                                if self.actions_contain_js(aa)? {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(false)
    }

    /// Check if an /AA (additional actions) dict contains any JavaScript actions.
    fn actions_contain_js(&self, aa_obj: &PdfObject) -> Result<bool> {
        let aa_dict = self.resolve_obj(aa_obj)?;
        if let Some(entries) = aa_dict.as_dict() {
            for (_key, val) in entries {
                let action = self.resolve_obj(val)?;
                if action.dict_get(b"S").and_then(|s| s.as_name()) == Some(b"JavaScript") {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Recursively scan AcroForm /Fields for JavaScript actions.
    fn form_fields_contain_js(&self, acro: &PdfObject) -> Result<bool> {
        if let Some(fields) = acro.dict_get(b"Fields") {
            let fields_arr = self.resolve_obj(fields)?;
            if let Some(arr) = fields_arr.as_array() {
                for field_ref in arr {
                    let field = self.resolve_obj(field_ref)?;
                    // Check /A
                    if let Some(a) = field.dict_get(b"A") {
                        let action = self.resolve_obj(a)?;
                        if action.dict_get(b"S").and_then(|s| s.as_name()) == Some(b"JavaScript") {
                            return Ok(true);
                        }
                    }
                    // Check /AA
                    if let Some(aa) = field.dict_get(b"AA") {
                        if self.actions_contain_js(aa)? {
                            return Ok(true);
                        }
                    }
                    // Recurse into /Kids
                    if field.dict_get(b"Kids").is_some() {
                        if self.form_fields_contain_js(&field)? {
                            return Ok(true);
                        }
                    }
                }
            }
        }
        Ok(false)
    }
}

// --- Helper functions ---

/// Extract a string value from a PDF dictionary, handling both literal and hex strings.
fn pdf_string(dict: &PdfObject, key: &[u8]) -> Option<String> {
    let val = dict.dict_get(key)?;
    match val {
        PdfObject::String(bytes) => {
            let s = decode_pdf_text(bytes);
            let trimmed = s.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        _ => None,
    }
}

/// Decode PDF text bytes to a Rust String.
///
/// PDF strings can be PDFDocEncoding (Latin-1 superset) or UTF-16BE
/// (indicated by BOM 0xFEFF).
fn decode_pdf_text(bytes: &[u8]) -> String {
    // UTF-16BE BOM
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let u16s: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&u16s);
    }

    // UTF-8 BOM
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        return String::from_utf8_lossy(&bytes[3..]).to_string();
    }

    // PDFDocEncoding - bytes 0x00-0x7F are ASCII, 0xA0-0xFF match Unicode,
    // but 0x80-0x9F map to specific Unicode codepoints per ISO 32000-1 Table D.2.
    let mut s = String::with_capacity(bytes.len());
    for &b in bytes {
        s.push(pdfdoc_byte_to_char(b));
    }
    s
}

/// PDFDocEncoding to Unicode lookup table (ISO 32000-1 Table D.2).
/// Entries that map 1:1 to Unicode (0x00-0x17, 0x20-0x7E, 0xA1-0xFF except 0xAD)
/// use 0 as sentinel - handled by fallback to `b as char`.
const PDFDOC_SPECIAL: [(u8, char); 43] = [
    // 0x18-0x1F: diacritical marks
    (0x18, '\u{02D8}'),
    (0x19, '\u{02C7}'),
    (0x1A, '\u{02C6}'),
    (0x1B, '\u{02D9}'),
    (0x1C, '\u{02DD}'),
    (0x1D, '\u{02DB}'),
    (0x1E, '\u{02DA}'),
    (0x1F, '\u{02DC}'),
    // 0x7F: undefined
    (0x7F, '\u{FFFD}'),
    // 0x80-0x9F: special mappings
    (0x80, '\u{2022}'),
    (0x81, '\u{2020}'),
    (0x82, '\u{2021}'),
    (0x83, '\u{2026}'),
    (0x84, '\u{2014}'),
    (0x85, '\u{2013}'),
    (0x86, '\u{0192}'),
    (0x87, '\u{2044}'),
    (0x88, '\u{2039}'),
    (0x89, '\u{203A}'),
    (0x8A, '\u{2212}'),
    (0x8B, '\u{2030}'),
    (0x8C, '\u{201E}'),
    (0x8D, '\u{201C}'),
    (0x8E, '\u{201D}'),
    (0x8F, '\u{2018}'),
    (0x90, '\u{2019}'),
    (0x91, '\u{201A}'),
    (0x92, '\u{2122}'),
    (0x93, '\u{FB01}'),
    (0x94, '\u{FB02}'),
    (0x95, '\u{0141}'),
    (0x96, '\u{0152}'),
    (0x97, '\u{0160}'),
    (0x98, '\u{0178}'),
    (0x99, '\u{017D}'),
    (0x9A, '\u{0131}'),
    (0x9B, '\u{0142}'),
    (0x9C, '\u{0153}'),
    (0x9D, '\u{0161}'),
    (0x9E, '\u{017E}'),
    (0x9F, '\u{FFFD}'),
    // 0xA0, 0xAD: differ from Latin-1
    (0xA0, '\u{20AC}'),
    (0xAD, '\u{FFFD}'),
];

/// Map a PDFDocEncoding byte to its Unicode character.
fn pdfdoc_byte_to_char(b: u8) -> char {
    for &(code, ch) in &PDFDOC_SPECIAL {
        if code == b {
            return ch;
        }
    }
    b as char
}

/// Parse a rectangle from a PdfObject, normalizing so x1<=x2, y1<=y2.
fn parse_rect(obj: Option<&PdfObject>) -> Option<[f64; 4]> {
    let arr = obj?.as_array()?;
    if arr.len() != 4 {
        return None;
    }
    let (mut x1, mut y1, mut x2, mut y2) = (
        arr[0].as_f64()?,
        arr[1].as_f64()?,
        arr[2].as_f64()?,
        arr[3].as_f64()?,
    );
    if x1 > x2 {
        std::mem::swap(&mut x1, &mut x2);
    }
    if y1 > y2 {
        std::mem::swap(&mut y1, &mut y2);
    }
    Some([x1, y1, x2, y2])
}

/// Parse a version string (bytes after `%PDF-`) into `major.minor` format.
/// Reads digits for major, then `.`, then digits for minor.
/// Returns "0.0" if parsing fails.
fn parse_version_string(data: &[u8]) -> Option<String> {
    // Read major digits
    let mut i = 0;
    while i < data.len() && data[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return Some("0.0".into());
    }
    let major = std::str::from_utf8(&data[..i]).ok()?;

    // Expect '.'
    if i >= data.len() || data[i] != b'.' {
        // No dot: e.g. "%PDF-14" -> "14.0"
        return Some(format!("{}.0", major));
    }
    i += 1;

    // Read minor digits
    let minor_start = i;
    while i < data.len() && data[i].is_ascii_digit() {
        i += 1;
    }
    if minor_start == i {
        return Some("0.0".into());
    }
    let minor: u32 = std::str::from_utf8(&data[minor_start..i])
        .ok()?
        .parse()
        .unwrap_or(0);

    Some(format!("{}.{}", major, minor))
}

/// Validate catalog /Version: must be exactly "N.N" (digit dot digit).
fn is_valid_catalog_version(v: &str) -> bool {
    let bytes = v.as_bytes();
    bytes.len() == 3 && bytes[0].is_ascii_digit() && bytes[1] == b'.' && bytes[2].is_ascii_digit()
}

/// Pick the higher PDF version string.
fn effective_version(header: &Option<String>, catalog: &Option<String>) -> Option<String> {
    match (header, catalog) {
        (Some(h), Some(c)) => {
            // Compare numerically: parse as f64
            let hv: f64 = h.parse().unwrap_or(0.0);
            let cv: f64 = c.parse().unwrap_or(0.0);
            if cv > hv {
                Some(c.clone())
            } else {
                Some(h.clone())
            }
        }
        (Some(h), None) => Some(h.clone()),
        (None, Some(c)) => Some(c.clone()),
        (None, None) => None,
    }
}

/// Simple XMP value extractor - looks for `<tag>value</tag>` or `tag="value"`.
fn extract_xmp_value(xmp: &str, tag: &str) -> Option<String> {
    // Try attribute form: tag="value"
    let attr_pattern = format!("{}=\"", tag);
    if let Some(start) = xmp.find(&attr_pattern) {
        let value_start = start + attr_pattern.len();
        if let Some(end) = xmp[value_start..].find('"') {
            return Some(xmp[value_start..value_start + end].to_string());
        }
    }

    // Try element form: <tag>value</tag>
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    if let Some(start) = xmp.find(&open) {
        let value_start = start + open.len();
        if let Some(end) = xmp[value_start..].find(&close) {
            return Some(xmp[value_start..value_start + end].trim().to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Build helpers ---

    fn build_pdf_with_info(info_entries: &str) -> Vec<u8> {
        let mut pdf = b"%PDF-1.7\n".to_vec();

        let obj1_offset = pdf.len();
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

        let obj3_offset = pdf.len();
        pdf.extend_from_slice(b"3 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] >>\nendobj\n");

        let obj4_offset = pdf.len();
        pdf.extend_from_slice(format!("4 0 obj\n<< {} >>\nendobj\n", info_entries).as_bytes());

        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 5\n");
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj1_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj2_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj3_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj4_offset, 0).as_bytes());
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size 5 /Root 1 0 R /Info 4 0 R >>\nstartxref\n{}\n%%EOF",
                xref_offset
            )
            .as_bytes(),
        );

        pdf
    }

    fn build_minimal_pdf() -> Vec<u8> {
        build_pdf_with_info(
            "/Title (Test Document) /Author (John Doe) /Creator (SiftX) /Producer (SiftX Library)",
        )
    }

    // --- PM1: Info dictionary ---

    #[test]
    fn pm1_info_dict() {
        let pdf = build_pdf_with_info(
            "/Title (My PDF) /Author (Jane) /Subject (Testing) /Keywords (pdf, test) /Creator (App) /Producer (Lib) /CreationDate (D:20240101120000) /ModDate (D:20240615)",
        );
        let doc = Document::parse(&pdf).unwrap();
        let meta = doc.metadata().unwrap();

        assert_eq!(meta.title.as_deref(), Some("My PDF"));
        assert_eq!(meta.author.as_deref(), Some("Jane"));
        assert_eq!(meta.subject.as_deref(), Some("Testing"));
        assert_eq!(meta.keywords.as_deref(), Some("pdf, test"));
        assert_eq!(meta.creator.as_deref(), Some("App"));
        assert_eq!(meta.producer.as_deref(), Some("Lib"));
        assert_eq!(meta.creation_date.as_deref(), Some("D:20240101120000"));
        assert_eq!(meta.mod_date.as_deref(), Some("D:20240615"));
    }

    #[test]
    fn pm1_info_trapped() {
        let pdf = build_pdf_with_info("/Title (Doc) /Trapped /True");
        let doc = Document::parse(&pdf).unwrap();
        let meta = doc.metadata().unwrap();
        assert_eq!(meta.trapped.as_deref(), Some("True"));
    }

    #[test]
    fn pm1_no_info() {
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let obj1_offset = pdf.len();
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 3\n");
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj1_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj2_offset, 0).as_bytes());
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
                xref_offset
            )
            .as_bytes(),
        );

        let doc = Document::parse(&pdf).unwrap();
        let meta = doc.metadata().unwrap();
        assert!(meta.title.is_none());
        assert!(meta.author.is_none());
    }

    // --- PM3: Version detection ---

    #[test]
    fn pm3_header_version() {
        let pdf = build_minimal_pdf();
        let doc = Document::parse(&pdf).unwrap();
        let meta = doc.metadata().unwrap();
        assert_eq!(meta.header_version.as_deref(), Some("1.7"));
        assert_eq!(meta.version.as_deref(), Some("1.7"));
    }

    #[test]
    fn pm3_catalog_version_higher() {
        let mut pdf = b"%PDF-1.5\n".to_vec();
        let obj1_offset = pdf.len();
        pdf.extend_from_slice(
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R /Version /2.0 >>\nendobj\n",
        );
        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 3\n");
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj1_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj2_offset, 0).as_bytes());
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
                xref_offset
            )
            .as_bytes(),
        );

        let doc = Document::parse(&pdf).unwrap();
        let meta = doc.metadata().unwrap();
        assert_eq!(meta.header_version.as_deref(), Some("1.5"));
        assert_eq!(meta.catalog_version.as_deref(), Some("2.0"));
        assert_eq!(meta.version.as_deref(), Some("2.0"));
    }

    // --- PM4: Page count ---

    #[test]
    fn pm4_page_count() {
        let pdf = build_minimal_pdf();
        let doc = Document::parse(&pdf).unwrap();
        let meta = doc.metadata().unwrap();
        assert_eq!(meta.page_count, 1);
    }

    // --- PM5: Page boxes ---

    #[test]
    fn pm5_page_boxes() {
        let pdf = build_minimal_pdf();
        let doc = Document::parse(&pdf).unwrap();
        let meta = doc.metadata().unwrap();

        assert_eq!(meta.pages.len(), 1);
        assert_eq!(meta.pages[0].media_box, [0.0, 0.0, 612.0, 792.0]);
        assert_eq!(meta.pages[0].crop_box, [0.0, 0.0, 612.0, 792.0]);
        assert!(meta.pages[0].bleed_box.is_none());
        assert!(meta.pages[0].trim_box.is_none());
        assert!(meta.pages[0].art_box.is_none());
    }

    // --- PM6: Rotation ---

    #[test]
    fn pm6_rotation() {
        let pdf = build_minimal_pdf();
        let doc = Document::parse(&pdf).unwrap();
        let meta = doc.metadata().unwrap();
        assert_eq!(meta.pages[0].rotate, 0);
    }

    // --- PM7: Subtype detection ---

    #[test]
    fn pm7_detect_pdfa_from_xmp() {
        let xmp =
            r#"<rdf:RDF><rdf:Description pdfaid:part="2" pdfaid:conformance="A" /></rdf:RDF>"#;
        let subtype = extract_xmp_subtype(xmp);
        assert_eq!(
            subtype,
            Some(PdfSubtype::PdfA {
                part: 2,
                conformance: "A".to_string()
            })
        );
    }

    #[test]
    fn pm7_detect_pdfx_from_xmp() {
        let xmp = r#"<rdf:RDF><pdfxid:GTS_PDFXVersion>PDF/X-4</pdfxid:GTS_PDFXVersion></rdf:RDF>"#;
        let subtype = extract_xmp_subtype(xmp);
        assert_eq!(subtype, Some(PdfSubtype::PdfX("PDF/X-4".to_string())));
    }

    #[test]
    fn pm7_detect_pdfua_from_xmp() {
        let xmp = r#"<rdf:RDF><rdf:Description pdfuaid:part="1" /></rdf:RDF>"#;
        let subtype = extract_xmp_subtype(xmp);
        assert_eq!(subtype, Some(PdfSubtype::PdfUA { part: 1 }));
    }

    #[test]
    fn pm7_no_subtype() {
        let xmp = r#"<rdf:RDF><rdf:Description /></rdf:RDF>"#;
        let subtype = extract_xmp_subtype(xmp);
        assert!(subtype.is_none());
    }

    // --- PM8: Encryption ---

    #[test]
    fn pm8_no_encryption() {
        let pdf = build_minimal_pdf();
        let doc = Document::parse(&pdf).unwrap();
        let meta = doc.metadata().unwrap();
        assert!(meta.encryption.is_none());
    }

    #[test]
    fn pm8_encryption_info() {
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let obj1_offset = pdf.len();
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
        let obj3_offset = pdf.len();
        pdf.extend_from_slice(
            b"3 0 obj\n<< /Filter /Standard /V 4 /Length 128 /R 4 /P -3904 >>\nendobj\n",
        );
        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 4\n");
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj1_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj2_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj3_offset, 0).as_bytes());
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size 4 /Root 1 0 R /Encrypt 3 0 R >>\nstartxref\n{}\n%%EOF",
                xref_offset
            )
            .as_bytes(),
        );

        let doc = Document::parse(&pdf).unwrap();
        let meta = doc.metadata().unwrap();
        let enc = meta.encryption.unwrap();
        assert!(enc.algorithm.contains("V4"));
        assert_eq!(enc.key_length, 128);
        assert_eq!(enc.revision, 4);
        assert_eq!(enc.permissions, -3904);
    }

    // --- PM9: Tagged PDF ---

    #[test]
    fn pm9_not_tagged() {
        let pdf = build_minimal_pdf();
        let doc = Document::parse(&pdf).unwrap();
        let meta = doc.metadata().unwrap();
        assert!(!meta.is_tagged);
    }

    #[test]
    fn pm9_tagged_mark_info() {
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let obj1_offset = pdf.len();
        pdf.extend_from_slice(
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R /MarkInfo << /Marked true >> >>\nendobj\n",
        );
        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 3\n");
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj1_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj2_offset, 0).as_bytes());
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
                xref_offset
            )
            .as_bytes(),
        );

        let doc = Document::parse(&pdf).unwrap();
        let meta = doc.metadata().unwrap();
        assert!(meta.is_tagged);
    }

    // --- PM10: JavaScript detection ---

    #[test]
    fn pm10_no_javascript() {
        let pdf = build_minimal_pdf();
        let doc = Document::parse(&pdf).unwrap();
        let meta = doc.metadata().unwrap();
        assert!(!meta.has_javascript);
    }

    #[test]
    fn pm10_javascript_in_names() {
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let obj1_offset = pdf.len();
        pdf.extend_from_slice(
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R /Names << /JavaScript << /Names [] >> >> >>\nendobj\n"
        );
        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 3\n");
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj1_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj2_offset, 0).as_bytes());
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
                xref_offset
            )
            .as_bytes(),
        );

        let doc = Document::parse(&pdf).unwrap();
        let meta = doc.metadata().unwrap();
        assert!(meta.has_javascript);
    }

    // --- PM11: Linearization ---

    #[test]
    fn pm11_not_linearized() {
        let pdf = build_minimal_pdf();
        let doc = Document::parse(&pdf).unwrap();
        let meta = doc.metadata().unwrap();
        assert!(!meta.is_linearized);
    }

    // --- Helper tests ---

    #[test]
    fn decode_pdf_text_ascii() {
        assert_eq!(decode_pdf_text(b"Hello"), "Hello");
    }

    #[test]
    fn decode_pdf_text_utf16be() {
        let mut bytes = vec![0xFE, 0xFF]; // BOM
        bytes.extend_from_slice(&[0x00, 0x48, 0x00, 0x69]); // "Hi"
        assert_eq!(decode_pdf_text(&bytes), "Hi");
    }

    #[test]
    fn effective_version_tests() {
        assert_eq!(
            effective_version(&Some("1.5".into()), &Some("2.0".into())),
            Some("2.0".into())
        );
        assert_eq!(
            effective_version(&Some("1.7".into()), &None),
            Some("1.7".into())
        );
        assert_eq!(effective_version(&None, &None), None);
    }

    #[test]
    fn extract_xmp_value_attr() {
        let xmp = r#"pdfaid:part="3""#;
        assert_eq!(extract_xmp_value(xmp, "pdfaid:part"), Some("3".to_string()));
    }

    #[test]
    fn extract_xmp_value_element() {
        let xmp = "<pdfaid:part>2</pdfaid:part>";
        assert_eq!(extract_xmp_value(xmp, "pdfaid:part"), Some("2".to_string()));
    }

    /// Helper to test subtype detection from XMP string directly.
    fn extract_xmp_subtype(xmp_str: &str) -> Option<PdfSubtype> {
        // PDF/A
        if let Some(part) = extract_xmp_value(xmp_str, "pdfaid:part") {
            let conformance =
                extract_xmp_value(xmp_str, "pdfaid:conformance").unwrap_or_else(|| "B".to_string());
            return Some(PdfSubtype::PdfA {
                part: part.parse().unwrap_or(1),
                conformance,
            });
        }
        if let Some(version) = extract_xmp_value(xmp_str, "pdfxid:GTS_PDFXVersion") {
            return Some(PdfSubtype::PdfX(version));
        }
        if let Some(part) = extract_xmp_value(xmp_str, "pdfuaid:part") {
            return Some(PdfSubtype::PdfUA {
                part: part.parse().unwrap_or(1),
            });
        }
        None
    }
}
