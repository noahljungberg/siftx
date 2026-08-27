//! PDF document structure (DS1-DS5).
//!
//! Provides access to catalog, page tree, page attributes, and linearization.
//! Per ISO 32000-2 §7.7.

use super::encrypt::SecurityHandler;
use super::object::{PdfObject, Ref};
use super::xref::{self, XRefTable};
use crate::core::{Error, Result};
use std::collections::HashSet;

/// Maximum page tree recursion depth.
const MAX_PAGE_TREE_DEPTH: u32 = 64;

/// A parsed PDF document.
pub struct Document<'a> {
    pub(super) data: &'a [u8],
    pub xref: XRefTable,
    /// Security handler for encrypted PDFs (set after parsing /Encrypt).
    security: Option<SecurityHandler>,
    /// Object number of the /Encrypt dict (skip decryption for this object).
    encrypt_obj_num: Option<u32>,
}

/// A single page with its resolved attributes.
#[derive(Debug, Clone)]
pub struct Page {
    /// Page dictionary (resolved).
    pub dict: PdfObject,
    /// MediaBox - required, possibly inherited.
    pub media_box: [f64; 4],
    /// CropBox - defaults to MediaBox if absent.
    pub crop_box: [f64; 4],
    /// Rotation in degrees (0, 90, 180, 270).
    pub rotate: i64,
    /// Resources dictionary reference or value.
    pub resources: Option<PdfObject>,
}

impl<'a> Document<'a> {
    /// Parse a PDF document from raw bytes.
    ///
    /// Builds the xref table and validates the trailer.
    /// If the PDF is encrypted, attempts authentication with an empty password.
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let xref = match xref::build_xref_table(data) {
            Ok(table) => table,
            Err(_e1) => {
                // Standard xref parsing failed. Try with nearby-xref fallback
                // (handles off-by-N startxref/Prev offsets in incremental updates),
                // but validate the result can resolve /Root before accepting it.
                match xref::build_xref_table_nearby(data) {
                    Ok(table) => {
                        // Validate: /Root must be resolvable
                        let valid = table
                            .trailer
                            .dict_get(b"Root")
                            .and_then(|r| r.as_ref())
                            .map(|r| xref::resolve_reference(data, &table, r, None).is_ok())
                            .unwrap_or(false);
                        if valid {
                            table
                        } else {
                            xref::reconstruct_xref(data)?
                        }
                    }
                    Err(_) => xref::reconstruct_xref(data)?,
                }
            }
        };

        let mut doc = Document {
            data,
            xref,
            security: None,
            encrypt_obj_num: None,
        };

        // Detect encryption and initialize security handler
        doc.init_security();

        Ok(doc)
    }

    /// Initialize security handler from /Encrypt dict in trailer.
    fn init_security(&mut self) {
        let encrypt_ref = match self.xref.trailer.dict_get(b"Encrypt") {
            Some(obj) => obj.clone(),
            None => return,
        };

        // Track the /Encrypt object number so we skip decryption for it
        self.encrypt_obj_num = encrypt_ref.as_ref().map(|r| r.num);

        // Resolve the /Encrypt dict (without decryption - it's never encrypted)
        let encrypt = match xref::resolve_deep(self.data, &self.xref, &encrypt_ref) {
            Ok(obj) => obj,
            Err(_) => return,
        };

        let mut handler = match SecurityHandler::from_encrypt_dict(&encrypt, &self.xref.trailer) {
            Ok(h) => h,
            Err(_) => return,
        };

        // Try empty password (covers permission-only encryption)
        handler.try_empty_password();

        self.security = Some(handler);
    }

    /// Authenticate with a password. Returns true if successful.
    pub fn authenticate(&mut self, password: &[u8]) -> bool {
        if let Some(ref mut handler) = self.security {
            if handler.is_authenticated() {
                return true;
            }
            handler.authenticate_user(password) || handler.authenticate_owner(password)
        } else {
            true // not encrypted
        }
    }

    /// Check if the document is encrypted.
    pub fn is_encrypted(&self) -> bool {
        self.security.is_some()
    }

    /// Check if authentication has succeeded (or document is not encrypted).
    pub fn is_authenticated(&self) -> bool {
        match &self.security {
            Some(handler) => handler.is_authenticated(),
            None => true,
        }
    }

    /// Get a reference to the security handler, if the document is encrypted.
    pub fn security_handler(&self) -> Option<&SecurityHandler> {
        self.security.as_ref()
    }

    /// Resolve an indirect reference, decrypting if needed.
    pub fn resolve(&self, reference: Ref) -> Result<PdfObject> {
        let sec = self.security.as_ref().filter(|h| h.is_authenticated());
        let obj = xref::resolve_reference(self.data, &self.xref, reference, sec)?;
        self.maybe_decrypt(obj, reference.num, reference.generation)
    }

    /// Resolve a PdfObject if it's a Ref, otherwise return it cloned.
    /// Decrypts strings and stream data if the document is encrypted.
    pub fn resolve_obj(&self, obj: &PdfObject) -> Result<PdfObject> {
        match obj {
            PdfObject::Ref(r) => self.resolve(*r),
            other => Ok(other.clone()),
        }
    }

    /// Decrypt a resolved object if we have an authenticated security handler.
    fn maybe_decrypt(&self, obj: PdfObject, obj_num: u32, gen_num: u16) -> Result<PdfObject> {
        let handler = match &self.security {
            Some(h) if h.is_authenticated() => h,
            _ => return Ok(obj),
        };

        // Never decrypt the /Encrypt dictionary itself
        if self.encrypt_obj_num == Some(obj_num) {
            return Ok(obj);
        }

        self.decrypt_object(handler, obj, obj_num, gen_num)
    }

    /// Recursively decrypt strings and stream data in an object.
    fn decrypt_object(
        &self,
        handler: &SecurityHandler,
        obj: PdfObject,
        obj_num: u32,
        gen_num: u16,
    ) -> Result<PdfObject> {
        match obj {
            PdfObject::String(data) => {
                let decrypted = handler
                    .decrypt_string(obj_num, gen_num, &data)
                    .unwrap_or(data);
                Ok(PdfObject::String(decrypted))
            }
            PdfObject::Array(items) => {
                let decrypted: Result<Vec<_>> = items
                    .into_iter()
                    .map(|item| self.decrypt_object(handler, item, obj_num, gen_num))
                    .collect();
                Ok(PdfObject::Array(decrypted?))
            }
            PdfObject::Dict(entries) => {
                let decrypted: Result<Vec<_>> = entries
                    .into_iter()
                    .map(|(key, val)| {
                        let dval = self.decrypt_object(handler, val, obj_num, gen_num)?;
                        Ok((key, dval))
                    })
                    .collect();
                Ok(PdfObject::Dict(decrypted?))
            }
            PdfObject::Stream { dict, data } => {
                // Decrypt stream data
                let decrypted_data = handler
                    .decrypt_stream(obj_num, gen_num, &data)
                    .unwrap_or(data);
                // Decrypt strings in the dict (but NOT the stream data keys like /Filter)
                let decrypted_dict: Result<Vec<_>> = dict
                    .into_iter()
                    .map(|(key, val)| {
                        let dval = self.decrypt_object(handler, val, obj_num, gen_num)?;
                        Ok((key, dval))
                    })
                    .collect();
                Ok(PdfObject::Stream {
                    dict: decrypted_dict?,
                    data: decrypted_data,
                })
            }
            // Other types (Bool, Int, Real, Name, Ref, Null) are never encrypted
            other => Ok(other),
        }
    }

    // --- DS1: Catalog ---

    /// DS1: Get the document catalog dictionary.
    pub fn catalog(&self) -> Result<PdfObject> {
        let root_ref = self
            .xref
            .trailer
            .dict_get(b"Root")
            .ok_or_else(|| Error::Format("trailer missing /Root".into()))?;

        self.resolve_obj(root_ref)
    }

    /// Get the set of OCG object references that are off by default.
    ///
    /// Reads `/OCProperties /D /OFF` from the catalog. Returns object numbers
    /// of OCGs that should be hidden. Used by content interpreter to skip
    /// content from invisible optional content groups.
    pub fn off_ocg_refs(&self) -> HashSet<(u32, u16)> {
        let mut off = HashSet::new();

        let catalog = match self.catalog() {
            Ok(c) => c,
            Err(_) => return off,
        };

        let ocprops = match catalog.dict_get(b"OCProperties") {
            Some(p) => match self.resolve_obj(p) {
                Ok(r) => r,
                Err(_) => return off,
            },
            None => return off,
        };

        let default_config = match ocprops.dict_get(b"D") {
            Some(d) => match self.resolve_obj(d) {
                Ok(r) => r,
                Err(_) => return off,
            },
            None => return off,
        };

        // Check BaseState - default is "ON"
        let base_off = default_config
            .dict_get(b"BaseState")
            .and_then(|bs| bs.as_name())
            .map(|n| n == b"OFF")
            .unwrap_or(false);

        if base_off {
            // All OCGs are off by default - collect all from /OCGs array
            if let Some(all_ocgs) = ocprops.dict_get(b"OCGs") {
                if let Ok(resolved) = self.resolve_obj(all_ocgs) {
                    if let Some(arr) = resolved.as_array() {
                        for item in arr {
                            if let Some(r) = item.as_ref() {
                                off.insert((r.num, r.generation));
                            }
                        }
                    }
                }
            }

            // Then remove any that are explicitly ON
            if let Some(on_arr) = default_config.dict_get(b"ON") {
                if let Ok(resolved) = self.resolve_obj(on_arr) {
                    if let Some(arr) = resolved.as_array() {
                        for item in arr {
                            if let Some(r) = item.as_ref() {
                                off.remove(&(r.num, r.generation));
                            }
                        }
                    }
                }
            }
        }

        // Collect explicitly OFF OCGs
        if let Some(off_arr) = default_config.dict_get(b"OFF") {
            if let Ok(resolved) = self.resolve_obj(off_arr) {
                if let Some(arr) = resolved.as_array() {
                    for item in arr {
                        if let Some(r) = item.as_ref() {
                            off.insert((r.num, r.generation));
                        }
                    }
                }
            }
        }

        off
    }

    // --- DS2 + DS3 + DS4: Page tree ---

    /// DS4: Get the total page count.
    pub fn page_count(&self) -> Result<u32> {
        let catalog = self.catalog()?;
        let pages_obj = catalog
            .dict_get(b"Pages")
            .ok_or_else(|| Error::Format("catalog missing /Pages".into()))?;
        let pages = self.resolve_obj(pages_obj)?;

        pages
            .dict_get(b"Count")
            .and_then(|c| c.as_int())
            .map(|c| c as u32)
            .ok_or_else(|| Error::Format("page tree root missing /Count".into()))
    }

    /// DS4: Get a page by zero-based index.
    pub fn page(&self, index: u32) -> Result<Page> {
        let pages = self.pages()?;
        pages
            .into_iter()
            .nth(index as usize)
            .ok_or_else(|| Error::Format(format!("page index {} out of range", index)))
    }

    /// DS2: Get all pages by traversing the page tree.
    pub fn pages(&self) -> Result<Vec<Page>> {
        let catalog = self.catalog()?;
        let pages_obj = catalog
            .dict_get(b"Pages")
            .ok_or_else(|| Error::Format("catalog missing /Pages".into()))?;
        let pages_root = self.resolve_obj(pages_obj)?;

        let mut result = Vec::new();
        let inherited = InheritedAttrs::default();
        let mut visited = HashSet::new();
        self.collect_pages(&pages_root, &inherited, &mut result, 0, &mut visited)?;
        Ok(result)
    }

    /// Recursively collect pages from the page tree.
    fn collect_pages(
        &self,
        node: &PdfObject,
        inherited: &InheritedAttrs,
        pages: &mut Vec<Page>,
        depth: u32,
        visited: &mut HashSet<u32>,
    ) -> Result<()> {
        if depth > MAX_PAGE_TREE_DEPTH {
            return Err(Error::Format("page tree too deep".into()));
        }

        let node_type = node
            .dict_get(b"Type")
            .and_then(|t| t.as_name_str())
            .unwrap_or("");

        // DS3: Merge inherited attributes from this node.
        // Resolve indirect references in MediaBox/CropBox before parsing.
        let resolve_rect_from = |key: &[u8]| -> Option<[f64; 4]> {
            let val = node.dict_get(key)?;
            let resolved = match val {
                PdfObject::Ref(_) => self.resolve_obj(val).ok()?,
                other => other.clone(),
            };
            // Resolve any indirect references inside the array elements
            let resolved = if let Some(arr) = resolved.as_array() {
                if arr.iter().any(|e| matches!(e, PdfObject::Ref(_))) {
                    let resolved_arr: Vec<PdfObject> = arr
                        .iter()
                        .map(|e| match e {
                            PdfObject::Ref(_) => self.resolve_obj(e).unwrap_or_else(|_| e.clone()),
                            _ => e.clone(),
                        })
                        .collect();
                    PdfObject::Array(resolved_arr)
                } else {
                    resolved
                }
            } else {
                resolved
            };
            parse_rect(Some(&resolved))
        };
        let attrs = InheritedAttrs {
            media_box: resolve_rect_from(b"MediaBox").or(inherited.media_box),
            crop_box: resolve_rect_from(b"CropBox").or(inherited.crop_box),
            rotate: node
                .dict_get(b"Rotate")
                .and_then(|r| r.as_int())
                .or(inherited.rotate),
            resources: node
                .dict_get(b"Resources")
                .cloned()
                .or(inherited.resources.clone()),
        };

        match node_type {
            "Pages" => {
                // Intermediate node - recurse into /Kids
                let kids = node
                    .dict_get(b"Kids")
                    .and_then(|k| k.as_array())
                    .ok_or_else(|| Error::Format("/Pages node missing /Kids".into()))?;

                for kid_obj in kids {
                    // Cycle detection via object number
                    if let Some(r) = kid_obj.as_ref() {
                        if !visited.insert(r.num) {
                            continue; // Already visited - skip cycle
                        }
                    }
                    let kid = self.resolve_obj(kid_obj)?;
                    self.collect_pages(&kid, &attrs, pages, depth + 1, visited)?;
                }
            }
            "Page" => {
                // Leaf node - build Page struct
                // attrs already has resolved MediaBox/CropBox from this node
                let media_box = attrs
                    .media_box
                    .filter(|b| (b[2] - b[0]).abs() > 0.0 && (b[3] - b[1]).abs() > 0.0)
                    .unwrap_or([0.0, 0.0, 612.0, 792.0]); // Default US Letter

                let mut crop_box = attrs.crop_box.unwrap_or(media_box);

                // Clamp CropBox to MediaBox (per Poppler / ISO 32000-1 §14.11.2)
                let cb_w = crop_box[2] - crop_box[0];
                let mb_w = media_box[2] - media_box[0];
                if cb_w > mb_w {
                    crop_box[0] = media_box[0];
                    crop_box[2] = media_box[2];
                }
                let cb_h = crop_box[3] - crop_box[1];
                let mb_h = media_box[3] - media_box[1];
                if cb_h > mb_h {
                    crop_box[1] = media_box[1];
                    crop_box[3] = media_box[3];
                }

                let rotate = attrs.rotate(node);

                let resources = node
                    .dict_get(b"Resources")
                    .cloned()
                    .or(attrs.resources.clone());

                pages.push(Page {
                    dict: node.clone(),
                    media_box,
                    crop_box,
                    rotate,
                    resources,
                });
            }
            _ => {
                // Unknown type - try treating as Pages (some malformed PDFs)
                if node.dict_get(b"Kids").is_some() {
                    let kids = node
                        .dict_get(b"Kids")
                        .and_then(|k| k.as_array())
                        .unwrap_or(&[]);
                    for kid_obj in kids {
                        if let Some(r) = kid_obj.as_ref() {
                            if !visited.insert(r.num) {
                                continue;
                            }
                        }
                        let kid = self.resolve_obj(kid_obj)?;
                        self.collect_pages(&kid, &attrs, pages, depth + 1, visited)?;
                    }
                }
            }
        }

        Ok(())
    }

    // --- DS5: Linearization detection ---

    /// DS5: Check if the PDF is linearized.
    ///
    /// A linearized PDF has `/Linearized` in its first indirect object,
    /// and the `/L` (file length) value must match the actual file size.
    pub fn is_linearized(&self) -> bool {
        if let Some(first_obj) = self.find_first_object() {
            if let Ok(obj) = self.resolve(first_obj) {
                if obj.dict_get(b"Linearized").is_some() {
                    // Validate /L matches actual file size (catches modified files)
                    if let Some(l) = obj.dict_get(b"L").and_then(|v| v.as_int()) {
                        return l as u64 == self.data.len() as u64;
                    }
                    // No /L key - still consider it linearized
                    return true;
                }
            }
        }
        false
    }

    /// Find the first indirect object by scanning for the lowest offset in the xref.
    fn find_first_object(&self) -> Option<Ref> {
        let mut best: Option<(u64, u32)> = None;

        for (idx, entry) in self.xref.entries.iter().enumerate() {
            if let Some(xref::XRefEntry::Uncompressed { offset, .. }) = entry {
                if idx > 0 {
                    // Skip object 0
                    if best.is_none() || *offset < best.unwrap().0 {
                        best = Some((*offset, idx as u32));
                    }
                }
            }
        }

        best.map(|(_, num)| Ref { num, generation: 0 })
    }

    /// Get the Info dictionary (document metadata).
    pub fn info(&self) -> Result<Option<PdfObject>> {
        match self.xref.trailer.dict_get(b"Info") {
            Some(info_ref) => Ok(Some(self.resolve_obj(info_ref)?)),
            None => Ok(None),
        }
    }
}

/// DS3: Inherited page attributes that propagate down the page tree.
#[derive(Debug, Clone, Default)]
struct InheritedAttrs {
    media_box: Option<[f64; 4]>,
    crop_box: Option<[f64; 4]>,
    rotate: Option<i64>,
    resources: Option<PdfObject>,
}

impl InheritedAttrs {
    /// Get Rotate from the page itself or inherited (default 0).
    fn rotate(&self, page: &PdfObject) -> i64 {
        page.dict_get(b"Rotate")
            .and_then(|r| r.as_int())
            .or(self.rotate)
            .unwrap_or(0)
    }
}

/// Parse a rectangle array `[llx lly urx ury]` -> `[f64; 4]`.
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
    // Normalize so x1<=x2, y1<=y2 (handles inverted coordinate boxes)
    if x1 > x2 {
        std::mem::swap(&mut x1, &mut x2);
    }
    if y1 > y2 {
        std::mem::swap(&mut y1, &mut y2);
    }
    Some([x1, y1, x2, y2])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid PDF with given pages.
    fn build_pdf_with_pages(page_dicts: &[&str], pages_extra: &str) -> Vec<u8> {
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let mut offsets: Vec<(u32, usize)> = Vec::new();

        // Object 1: Catalog
        offsets.push((1, pdf.len()));
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        // Page objects start at 3
        let mut page_refs = String::new();
        for (i, page_dict) in page_dicts.iter().enumerate() {
            let obj_num = 3 + i as u32;
            offsets.push((obj_num, pdf.len()));
            pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", obj_num, page_dict).as_bytes());
            page_refs.push_str(&format!("{} 0 R ", obj_num));
        }

        // Object 2: Pages
        offsets.push((2, pdf.len()));
        pdf.extend_from_slice(
            format!(
                "2 0 obj\n<< /Type /Pages /Kids [{}] /Count {} {} >>\nendobj\n",
                page_refs.trim(),
                page_dicts.len(),
                pages_extra,
            )
            .as_bytes(),
        );

        // Write xref
        let xref_offset = pdf.len();
        let max_obj = offsets.iter().map(|(n, _)| *n).max().unwrap_or(0);
        pdf.extend_from_slice(format!("xref\n0 {}\n", max_obj + 1).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");

        for obj_num in 1..=max_obj {
            if let Some((_, offset)) = offsets.iter().find(|(n, _)| *n == obj_num) {
                pdf.extend_from_slice(format!("{:010} {:05} n \n", offset, 0).as_bytes());
            } else {
                pdf.extend_from_slice(b"0000000000 00000 f \n");
            }
        }

        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
                max_obj + 1,
                xref_offset
            )
            .as_bytes(),
        );

        pdf
    }

    // --- DS1: Catalog ---

    #[test]
    fn ds1_catalog() {
        let pdf = build_pdf_with_pages(&["<< /Type /Page /MediaBox [0 0 612 792] >>"], "");
        let doc = Document::parse(&pdf).unwrap();
        let catalog = doc.catalog().unwrap();
        assert_eq!(
            catalog.dict_get(b"Type"),
            Some(&PdfObject::Name(b"Catalog".to_vec()))
        );
        assert!(catalog.dict_get(b"Pages").is_some());
    }

    // --- DS2: Page tree ---

    #[test]
    fn ds2_single_page() {
        let pdf = build_pdf_with_pages(&["<< /Type /Page /MediaBox [0 0 612 792] >>"], "");
        let doc = Document::parse(&pdf).unwrap();
        let pages = doc.pages().unwrap();
        assert_eq!(pages.len(), 1);
    }

    #[test]
    fn ds2_multiple_pages() {
        let pdf = build_pdf_with_pages(
            &[
                "<< /Type /Page /MediaBox [0 0 612 792] >>",
                "<< /Type /Page /MediaBox [0 0 595 842] >>",
                "<< /Type /Page /MediaBox [0 0 612 792] >>",
            ],
            "",
        );
        let doc = Document::parse(&pdf).unwrap();
        let pages = doc.pages().unwrap();
        assert_eq!(pages.len(), 3);
    }

    // --- DS3: Inherited attributes ---

    #[test]
    fn ds3_inherited_media_box() {
        // MediaBox on /Pages node, not on individual pages
        let pdf = build_pdf_with_pages(
            &[
                "<< /Type /Page >>",                         // No MediaBox - should inherit
                "<< /Type /Page /MediaBox [0 0 100 200] >>", // Override
            ],
            "/MediaBox [0 0 612 792]",
        );
        let doc = Document::parse(&pdf).unwrap();
        let pages = doc.pages().unwrap();

        // Page 0 inherits from parent
        assert_eq!(pages[0].media_box, [0.0, 0.0, 612.0, 792.0]);
        // Page 1 has its own
        assert_eq!(pages[1].media_box, [0.0, 0.0, 100.0, 200.0]);
    }

    #[test]
    fn ds3_inherited_rotate() {
        let pdf = build_pdf_with_pages(
            &[
                "<< /Type /Page /MediaBox [0 0 612 792] >>",
                "<< /Type /Page /MediaBox [0 0 612 792] /Rotate 180 >>",
            ],
            "/Rotate 90",
        );
        let doc = Document::parse(&pdf).unwrap();
        let pages = doc.pages().unwrap();

        assert_eq!(pages[0].rotate, 90); // inherited
        assert_eq!(pages[1].rotate, 180); // overridden
    }

    #[test]
    fn ds3_crop_box_defaults_to_media_box() {
        let pdf = build_pdf_with_pages(&["<< /Type /Page /MediaBox [0 0 612 792] >>"], "");
        let doc = Document::parse(&pdf).unwrap();
        let pages = doc.pages().unwrap();
        assert_eq!(pages[0].crop_box, pages[0].media_box);
    }

    // --- DS4: Page count and access ---

    #[test]
    fn ds4_page_count() {
        let pdf = build_pdf_with_pages(
            &[
                "<< /Type /Page /MediaBox [0 0 612 792] >>",
                "<< /Type /Page /MediaBox [0 0 612 792] >>",
            ],
            "",
        );
        let doc = Document::parse(&pdf).unwrap();
        assert_eq!(doc.page_count().unwrap(), 2);
    }

    #[test]
    fn ds4_page_by_index() {
        let pdf = build_pdf_with_pages(
            &[
                "<< /Type /Page /MediaBox [0 0 100 200] >>",
                "<< /Type /Page /MediaBox [0 0 300 400] >>",
            ],
            "",
        );
        let doc = Document::parse(&pdf).unwrap();

        let p0 = doc.page(0).unwrap();
        assert_eq!(p0.media_box, [0.0, 0.0, 100.0, 200.0]);

        let p1 = doc.page(1).unwrap();
        assert_eq!(p1.media_box, [0.0, 0.0, 300.0, 400.0]);

        assert!(doc.page(2).is_err());
    }

    // --- DS5: Linearization detection ---

    #[test]
    fn ds5_not_linearized() {
        let pdf = build_pdf_with_pages(&["<< /Type /Page /MediaBox [0 0 612 792] >>"], "");
        let doc = Document::parse(&pdf).unwrap();
        assert!(!doc.is_linearized());
    }

    #[test]
    fn ds5_linearized() {
        // Build a PDF where the first object has /Linearized
        let mut pdf = b"%PDF-1.7\n".to_vec();

        // Use a placeholder for /L, then patch it with actual file length.
        // /L is zero-padded to allow in-place replacement.
        let obj1_offset = pdf.len();
        let l_placeholder = b"1 0 obj\n<< /Linearized 1.0 /L 00000 /O 3 /E 500 /N 1 /T 900 /H [100 50] >>\nendobj\n";
        pdf.extend_from_slice(l_placeholder);

        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Catalog /Pages 3 0 R >>\nendobj\n");

        let obj3_offset = pdf.len();
        pdf.extend_from_slice(b"3 0 obj\n<< /Type /Pages /Kids [4 0 R] /Count 1 >>\nendobj\n");

        let obj4_offset = pdf.len();
        pdf.extend_from_slice(b"4 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] >>\nendobj\n");

        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 5\n");
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj1_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj2_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj3_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj4_offset, 0).as_bytes());
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size 5 /Root 2 0 R >>\nstartxref\n{}\n%%EOF",
                xref_offset
            )
            .as_bytes(),
        );

        // Patch /L with actual file length
        let file_len = pdf.len();
        let l_str = format!("{:05}", file_len);
        let l_pos = pdf.windows(5).position(|w| w == b"00000").unwrap();
        pdf[l_pos..l_pos + 5].copy_from_slice(l_str.as_bytes());

        let doc = Document::parse(&pdf).unwrap();
        assert!(doc.is_linearized());
    }

    // --- Info dictionary ---

    #[test]
    fn info_dict() {
        let mut pdf = b"%PDF-1.7\n".to_vec();

        let obj1_offset = pdf.len();
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");

        let obj3_offset = pdf.len();
        pdf.extend_from_slice(b"3 0 obj\n<< /Author (John Doe) /Title (Test PDF) >>\nendobj\n");

        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 4\n");
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj1_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj2_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj3_offset, 0).as_bytes());
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size 4 /Root 1 0 R /Info 3 0 R >>\nstartxref\n{}\n%%EOF",
                xref_offset
            )
            .as_bytes(),
        );

        let doc = Document::parse(&pdf).unwrap();
        let info = doc.info().unwrap().unwrap();
        assert_eq!(
            info.dict_get(b"Author"),
            Some(&PdfObject::String(b"John Doe".to_vec()))
        );
        assert_eq!(
            info.dict_get(b"Title"),
            Some(&PdfObject::String(b"Test PDF".to_vec()))
        );
    }

    // --- parse_rect ---

    #[test]
    fn parse_rect_valid() {
        let arr = PdfObject::Array(vec![
            PdfObject::Int(0),
            PdfObject::Int(0),
            PdfObject::Real(612.0),
            PdfObject::Real(792.0),
        ]);
        assert_eq!(parse_rect(Some(&arr)), Some([0.0, 0.0, 612.0, 792.0]));
    }

    #[test]
    fn parse_rect_int_coercion() {
        let arr = PdfObject::Array(vec![
            PdfObject::Int(0),
            PdfObject::Int(0),
            PdfObject::Int(595),
            PdfObject::Int(842),
        ]);
        assert_eq!(parse_rect(Some(&arr)), Some([0.0, 0.0, 595.0, 842.0]));
    }

    #[test]
    fn parse_rect_wrong_length() {
        let arr = PdfObject::Array(vec![PdfObject::Int(0), PdfObject::Int(0)]);
        assert_eq!(parse_rect(Some(&arr)), None);
    }

    #[test]
    fn parse_rect_none() {
        assert_eq!(parse_rect(None), None);
    }
}
