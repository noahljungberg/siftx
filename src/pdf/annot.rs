//! PDF form field and annotation extraction (AV6-AV7).
//!
//! - AV6: AcroForm field tree traversal and field extraction
//! - AV7: Annotation extraction from page /Annots arrays
//! Per ISO 32000-2 §12.5 (annotations) and §12.7 (forms).

use super::document::Document;
use super::object::PdfObject;
use crate::core::{Error, Result};

// ---------------------------------------------------------------------------
// AV7: Annotation types
// ---------------------------------------------------------------------------

/// AV7: Annotation type enum (ISO 32000-2 §12.5.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationType {
    Text,
    Link,
    FreeText,
    Line,
    Square,
    Circle,
    Polygon,
    PolyLine,
    Highlight,
    Underline,
    Squiggly,
    StrikeOut,
    Stamp,
    Caret,
    Ink,
    Popup,
    FileAttachment,
    Sound,
    Movie,
    Widget,
    Screen,
    PrinterMark,
    TrapNet,
    Watermark,
    ThreeD,
    Redact,
    RichMedia,
    Projection,
    Unknown,
}

impl AnnotationType {
    fn from_name(name: &str) -> Self {
        match name {
            "Text" => Self::Text,
            "Link" => Self::Link,
            "FreeText" => Self::FreeText,
            "Line" => Self::Line,
            "Square" => Self::Square,
            "Circle" => Self::Circle,
            "Polygon" => Self::Polygon,
            "PolyLine" => Self::PolyLine,
            "Highlight" => Self::Highlight,
            "Underline" => Self::Underline,
            "Squiggly" => Self::Squiggly,
            "StrikeOut" => Self::StrikeOut,
            "Stamp" => Self::Stamp,
            "Caret" => Self::Caret,
            "Ink" => Self::Ink,
            "Popup" => Self::Popup,
            "FileAttachment" => Self::FileAttachment,
            "Sound" => Self::Sound,
            "Movie" => Self::Movie,
            "Widget" => Self::Widget,
            "Screen" => Self::Screen,
            "PrinterMark" => Self::PrinterMark,
            "TrapNet" => Self::TrapNet,
            "Watermark" => Self::Watermark,
            "3D" => Self::ThreeD,
            "Redact" => Self::Redact,
            "RichMedia" => Self::RichMedia,
            "Projection" => Self::Projection,
            _ => Self::Unknown,
        }
    }

    /// Get the standard name for this annotation type.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Link => "Link",
            Self::FreeText => "FreeText",
            Self::Line => "Line",
            Self::Square => "Square",
            Self::Circle => "Circle",
            Self::Polygon => "Polygon",
            Self::PolyLine => "PolyLine",
            Self::Highlight => "Highlight",
            Self::Underline => "Underline",
            Self::Squiggly => "Squiggly",
            Self::StrikeOut => "StrikeOut",
            Self::Stamp => "Stamp",
            Self::Caret => "Caret",
            Self::Ink => "Ink",
            Self::Popup => "Popup",
            Self::FileAttachment => "FileAttachment",
            Self::Sound => "Sound",
            Self::Movie => "Movie",
            Self::Widget => "Widget",
            Self::Screen => "Screen",
            Self::PrinterMark => "PrinterMark",
            Self::TrapNet => "TrapNet",
            Self::Watermark => "Watermark",
            Self::ThreeD => "3D",
            Self::Redact => "Redact",
            Self::RichMedia => "RichMedia",
            Self::Projection => "Projection",
            Self::Unknown => "Unknown",
        }
    }
}

/// AV7: A single annotation.
#[derive(Debug, Clone)]
pub struct Annotation {
    /// Annotation type.
    pub annot_type: AnnotationType,
    /// Rectangle: [llx, lly, urx, ury].
    pub rect: [f64; 4],
    /// /Contents - text content or alt text.
    pub contents: Option<String>,
    /// /NM - unique name.
    pub name: Option<String>,
    /// /M - modification date.
    pub modified: Option<String>,
    /// /F - annotation flags.
    pub flags: u32,
    /// /C - color array (0-3 components).
    pub color: Option<Vec<f64>>,
    /// /Border - border style [horizontal_radius, vertical_radius, width].
    pub border: Option<[f64; 3]>,
    /// Destination URI or named destination (for Link annotations).
    pub dest: Option<String>,
    /// Page index (zero-based).
    pub page_index: usize,
    /// Whether an appearance stream (/AP) exists.
    pub has_appearance: bool,
}

// ---------------------------------------------------------------------------
// AV6: Form fields
// ---------------------------------------------------------------------------

/// AV6: Form field type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// /FT /Tx - text input.
    Text,
    /// /FT /Btn - button (checkbox, radio, push button).
    Button,
    /// /FT /Ch - choice (list box, combo box).
    Choice,
    /// /FT /Sig - digital signature.
    Signature,
    /// Unknown field type.
    Unknown,
}

impl FieldType {
    fn from_name(name: &[u8]) -> Self {
        match name {
            b"Tx" => Self::Text,
            b"Btn" => Self::Button,
            b"Ch" => Self::Choice,
            b"Sig" => Self::Signature,
            _ => Self::Unknown,
        }
    }

    /// Get the standard name for this field type.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Button => "Button",
            Self::Choice => "Choice",
            Self::Signature => "Signature",
            Self::Unknown => "Unknown",
        }
    }
}

/// AV6: A single form field.
#[derive(Debug, Clone)]
pub struct FormField {
    /// Field type.
    pub field_type: FieldType,
    /// Partial field name (/T).
    pub partial_name: Option<String>,
    /// Fully qualified name (parent.child.child).
    pub full_name: String,
    /// Current value (/V).
    pub value: Option<String>,
    /// Default value (/DV).
    pub default_value: Option<String>,
    /// Field flags (/Ff).
    pub flags: u32,
    /// Options list (/Opt) for choice fields.
    pub options: Vec<String>,
    /// Widget annotation rectangle.
    pub rect: Option<[f64; 4]>,
    /// Child fields (for non-terminal field tree nodes).
    pub children: Vec<FormField>,
}

/// AV6: Top-level AcroForm data.
#[derive(Debug, Clone)]
pub struct AcroForm {
    /// All top-level fields.
    pub fields: Vec<FormField>,
    /// /NeedAppearances flag.
    pub need_appearances: bool,
    /// /SigFlags (signature flags).
    pub sig_flags: u32,
}

// ---------------------------------------------------------------------------
// Implementation on Document
// ---------------------------------------------------------------------------

impl<'a> Document<'a> {
    // --- AV6: Form fields ---

    /// AV6: Extract the AcroForm (interactive form) from the document.
    ///
    /// Returns `None` if the document has no /AcroForm.
    pub fn acro_form(&self) -> Result<Option<AcroForm>> {
        let catalog = self.catalog()?;

        let form_ref = match catalog.dict_get(b"AcroForm") {
            Some(r) => r,
            None => return Ok(None),
        };

        let form_dict = self.resolve_obj(form_ref)?;

        let need_appearances = form_dict
            .dict_get(b"NeedAppearances")
            .and_then(|n| n.as_bool())
            .unwrap_or(false);

        let sig_flags = form_dict
            .dict_get(b"SigFlags")
            .and_then(|s| s.as_int())
            .unwrap_or(0) as u32;

        // Parse /Fields array
        let fields_array = form_dict
            .dict_get(b"Fields")
            .and_then(|f| f.as_array())
            .unwrap_or(&[]);

        let mut fields = Vec::new();
        for field_ref in fields_array {
            let field_obj = self.resolve_obj(field_ref)?;
            if let Some(field) = self.parse_form_field(&field_obj, "", None, 0)? {
                fields.push(field);
            }
        }

        Ok(Some(AcroForm {
            fields,
            need_appearances,
            sig_flags,
        }))
    }

    /// Parse a single form field from its dictionary.
    fn parse_form_field(
        &self,
        obj: &PdfObject,
        parent_name: &str,
        inherited_ft: Option<FieldType>,
        depth: u32,
    ) -> Result<Option<FormField>> {
        if depth > 50 {
            return Ok(None);
        }

        // Field type: can be on this field or inherited from parent
        let field_type = obj
            .dict_get(b"FT")
            .and_then(|ft| ft.as_name())
            .map(FieldType::from_name)
            .or(inherited_ft)
            .unwrap_or(FieldType::Unknown);

        // Partial name (/T)
        let partial_name = obj
            .dict_get(b"T")
            .and_then(|t| t.as_string())
            .map(|s| String::from_utf8_lossy(s).to_string());

        // Full name
        let full_name = if let Some(ref pn) = partial_name {
            if parent_name.is_empty() {
                pn.clone()
            } else {
                format!("{}.{}", parent_name, pn)
            }
        } else {
            parent_name.to_string()
        };

        // Value (/V)
        let value = extract_field_value(obj.dict_get(b"V"));

        // Default value (/DV)
        let default_value = extract_field_value(obj.dict_get(b"DV"));

        // Flags (/Ff)
        let flags = obj.dict_get(b"Ff").and_then(|f| f.as_int()).unwrap_or(0) as u32;

        // Options (/Opt)
        let options = obj
            .dict_get(b"Opt")
            .and_then(|o| o.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        match item {
                            PdfObject::String(s) => Some(String::from_utf8_lossy(s).to_string()),
                            PdfObject::Array(pair) if pair.len() >= 2 => {
                                // [export_value, display_value]
                                pair.get(1)
                                    .and_then(|v| v.as_string())
                                    .map(|s| String::from_utf8_lossy(s).to_string())
                            }
                            _ => None,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Widget rect (/Rect) - present if field is merged with widget annotation
        let rect = parse_rect(obj.dict_get(b"Rect"));

        // Children (/Kids)
        let mut children = Vec::new();
        if let Some(kids) = obj.dict_get(b"Kids").and_then(|k| k.as_array()) {
            for kid_ref in kids {
                let kid_obj = self.resolve_obj(kid_ref)?;
                // Check if kid is a widget annotation (has /Subtype /Widget)
                // or another field (has /T or /Kids)
                let is_widget_only = kid_obj
                    .dict_get(b"Subtype")
                    .and_then(|s| s.as_name_str())
                    .map(|s| s == "Widget")
                    .unwrap_or(false)
                    && kid_obj.dict_get(b"T").is_none()
                    && kid_obj.dict_get(b"Kids").is_none();

                if !is_widget_only {
                    if let Some(child) =
                        self.parse_form_field(&kid_obj, &full_name, Some(field_type), depth + 1)?
                    {
                        children.push(child);
                    }
                }
            }
        }

        Ok(Some(FormField {
            field_type,
            partial_name,
            full_name,
            value,
            default_value,
            flags,
            options,
            rect,
            children,
        }))
    }

    // --- AV7: Annotations ---

    /// AV7: Extract annotations from a specific page.
    pub fn annotations(&self, page_index: usize) -> Result<Vec<Annotation>> {
        let pages = self.pages()?;
        let page = pages
            .get(page_index)
            .ok_or_else(|| Error::Format(format!("page {} out of range", page_index)))?;

        let annots_ref = match page.dict.dict_get(b"Annots") {
            Some(a) => a,
            None => return Ok(Vec::new()),
        };

        let annots_obj = self.resolve_obj(annots_ref)?;
        let annots_array = annots_obj
            .as_array()
            .ok_or_else(|| Error::Format("/Annots is not an array".into()))?;

        let mut annotations = Vec::new();

        for annot_ref in annots_array {
            let annot_obj = self.resolve_obj(annot_ref)?;
            if let Some(annot) = self.parse_annotation(&annot_obj, page_index)? {
                annotations.push(annot);
            }
        }

        Ok(annotations)
    }

    /// AV7: Extract annotations from all pages.
    pub fn all_annotations(&self) -> Result<Vec<Annotation>> {
        let page_count = self.page_count()? as usize;
        let mut all = Vec::new();

        for i in 0..page_count {
            let mut page_annots = self.annotations(i)?;
            all.append(&mut page_annots);
        }

        Ok(all)
    }

    /// Parse a single annotation dictionary.
    fn parse_annotation(&self, obj: &PdfObject, page_index: usize) -> Result<Option<Annotation>> {
        // /Subtype is required
        let subtype = match obj.dict_get(b"Subtype").and_then(|s| s.as_name_str()) {
            Some(s) => s,
            None => return Ok(None),
        };

        let annot_type = AnnotationType::from_name(subtype);

        // /Rect - required
        let rect = parse_rect(obj.dict_get(b"Rect")).unwrap_or([0.0, 0.0, 0.0, 0.0]);

        // /Contents
        let contents = obj
            .dict_get(b"Contents")
            .and_then(|c| c.as_string())
            .map(|s| String::from_utf8_lossy(s).to_string());

        // /NM
        let name = obj
            .dict_get(b"NM")
            .and_then(|n| n.as_string())
            .map(|s| String::from_utf8_lossy(s).to_string());

        // /M
        let modified = obj
            .dict_get(b"M")
            .and_then(|m| m.as_string())
            .map(|s| String::from_utf8_lossy(s).to_string());

        // /F
        let flags = obj.dict_get(b"F").and_then(|f| f.as_int()).unwrap_or(0) as u32;

        // /C
        let color = obj
            .dict_get(b"C")
            .and_then(|c| c.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect());

        // /Border
        let border = obj
            .dict_get(b"Border")
            .and_then(|b| b.as_array())
            .and_then(|arr| {
                if arr.len() >= 3 {
                    Some([
                        arr[0].as_f64().unwrap_or(0.0),
                        arr[1].as_f64().unwrap_or(0.0),
                        arr[2].as_f64().unwrap_or(0.0),
                    ])
                } else {
                    None
                }
            });

        // /Dest or /A for Link annotations
        let dest = self.extract_annotation_dest(obj);

        // /AP
        let has_appearance = obj.dict_get(b"AP").is_some();

        Ok(Some(Annotation {
            annot_type,
            rect,
            contents,
            name,
            modified,
            flags,
            color,
            border,
            dest,
            page_index,
            has_appearance,
        }))
    }

    /// Extract destination from a Link annotation.
    fn extract_annotation_dest(&self, obj: &PdfObject) -> Option<String> {
        // Try /Dest first
        if let Some(dest) = obj.dict_get(b"Dest") {
            return match dest {
                PdfObject::String(s) => Some(String::from_utf8_lossy(s).to_string()),
                PdfObject::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                _ => None,
            };
        }

        // Try /A (action dictionary)
        if let Some(action_ref) = obj.dict_get(b"A") {
            if let Ok(action) = self.resolve_obj(action_ref) {
                let action_type = action
                    .dict_get(b"S")
                    .and_then(|s| s.as_name_str())
                    .unwrap_or("");

                match action_type {
                    "URI" => {
                        return action
                            .dict_get(b"URI")
                            .and_then(|u| u.as_string())
                            .map(|s| String::from_utf8_lossy(s).to_string());
                    }
                    "GoTo" => {
                        return action
                            .dict_get(b"D")
                            .and_then(|d| d.as_string())
                            .map(|s| String::from_utf8_lossy(s).to_string());
                    }
                    "GoToR" => {
                        return action
                            .dict_get(b"F")
                            .and_then(|f| f.as_string())
                            .map(|s| String::from_utf8_lossy(s).to_string());
                    }
                    _ => {}
                }
            }
        }

        None
    }
}

// --- Helpers ---

/// Extract a form field value from a PdfObject.
fn extract_field_value(obj: Option<&PdfObject>) -> Option<String> {
    match obj? {
        PdfObject::String(s) => Some(String::from_utf8_lossy(s).to_string()),
        PdfObject::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
        PdfObject::Int(i) => Some(i.to_string()),
        PdfObject::Real(r) => Some(r.to_string()),
        PdfObject::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Parse a rectangle [llx, lly, urx, ury].
fn parse_rect(obj: Option<&PdfObject>) -> Option<[f64; 4]> {
    let arr = obj?.as_array()?;
    if arr.len() != 4 {
        return None;
    }
    Some([
        arr[0].as_f64()?,
        arr[1].as_f64()?,
        arr[2].as_f64()?,
        arr[3].as_f64()?,
    ])
}

impl FormField {
    /// Total field count (this field + descendants).
    pub fn field_count(&self) -> usize {
        1 + self.children.iter().map(|c| c.field_count()).sum::<usize>()
    }

    /// Is this a read-only field? (bit 1 of /Ff).
    pub fn is_read_only(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Is this a required field? (bit 2 of /Ff).
    pub fn is_required(&self) -> bool {
        self.flags & 2 != 0
    }
}

impl AcroForm {
    /// Total number of fields (recursive).
    pub fn total_field_count(&self) -> usize {
        self.fields.iter().map(|f| f.field_count()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a PDF with annotations on page 1.
    fn build_pdf_with_annots(annot_objects: &[(u32, &str)]) -> Vec<u8> {
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let mut offsets: Vec<(u32, usize)> = Vec::new();

        // Build annot ref string
        let annot_refs: String = annot_objects
            .iter()
            .map(|(num, _)| format!("{} 0 R", num))
            .collect::<Vec<_>>()
            .join(" ");

        // Object 1: Catalog
        offsets.push((1, pdf.len()));
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        // Object 2: Pages
        offsets.push((2, pdf.len()));
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

        // Object 3: Page with /Annots
        offsets.push((3, pdf.len()));
        pdf.extend_from_slice(format!(
            "3 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] /Parent 2 0 R /Annots [{}] >>\nendobj\n",
            annot_refs
        ).as_bytes());

        // Annotation objects
        for &(num, body) in annot_objects {
            offsets.push((num, pdf.len()));
            pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", num, body).as_bytes());
        }

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

    /// Build a PDF with an AcroForm.
    fn build_pdf_with_form(form_entries: &str, extra_objects: &[(u32, &str)]) -> Vec<u8> {
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let mut offsets: Vec<(u32, usize)> = Vec::new();

        // Object 1: Catalog with /AcroForm
        offsets.push((1, pdf.len()));
        pdf.extend_from_slice(
            format!(
                "1 0 obj\n<< /Type /Catalog /Pages 2 0 R /AcroForm << {} >> >>\nendobj\n",
                form_entries
            )
            .as_bytes(),
        );

        // Object 2: Pages
        offsets.push((2, pdf.len()));
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

        // Object 3: Page
        offsets.push((3, pdf.len()));
        pdf.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] /Parent 2 0 R >>\nendobj\n",
        );

        // Extra objects
        for &(num, body) in extra_objects {
            offsets.push((num, pdf.len()));
            pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", num, body).as_bytes());
        }

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

    // ====================================================================
    // AV7: Annotation tests
    // ====================================================================

    #[test]
    fn av7_no_annotations() {
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let obj1_offset = pdf.len();
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        let obj2_offset = pdf.len();
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
        let obj3_offset = pdf.len();
        pdf.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] /Parent 2 0 R >>\nendobj\n",
        );
        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 4\n");
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj1_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj2_offset, 0).as_bytes());
        pdf.extend_from_slice(format!("{:010} {:05} n \n", obj3_offset, 0).as_bytes());
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
                xref_offset
            )
            .as_bytes(),
        );

        let doc = Document::parse(&pdf).unwrap();
        let annots = doc.annotations(0).unwrap();
        assert!(annots.is_empty());
    }

    #[test]
    fn av7_text_annotation() {
        let pdf = build_pdf_with_annots(&[(
            4,
            "<< /Type /Annot /Subtype /Text /Rect [100 200 200 300] /Contents (A note) >>",
        )]);
        let doc = Document::parse(&pdf).unwrap();
        let annots = doc.annotations(0).unwrap();

        assert_eq!(annots.len(), 1);
        assert_eq!(annots[0].annot_type, AnnotationType::Text);
        assert_eq!(annots[0].rect, [100.0, 200.0, 200.0, 300.0]);
        assert_eq!(annots[0].contents.as_deref(), Some("A note"));
        assert_eq!(annots[0].page_index, 0);
    }

    #[test]
    fn av7_link_annotation_with_uri() {
        let pdf = build_pdf_with_annots(&[(
            4,
            "<< /Type /Annot /Subtype /Link /Rect [0 0 100 20] /A << /S /URI /URI (https://example.com) >> >>",
        )]);
        let doc = Document::parse(&pdf).unwrap();
        let annots = doc.annotations(0).unwrap();

        assert_eq!(annots.len(), 1);
        assert_eq!(annots[0].annot_type, AnnotationType::Link);
        assert_eq!(annots[0].dest.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn av7_link_annotation_with_dest() {
        let pdf = build_pdf_with_annots(&[(
            4,
            "<< /Type /Annot /Subtype /Link /Rect [0 0 50 10] /Dest (chapter1) >>",
        )]);
        let doc = Document::parse(&pdf).unwrap();
        let annots = doc.annotations(0).unwrap();

        assert_eq!(annots[0].dest.as_deref(), Some("chapter1"));
    }

    #[test]
    fn av7_highlight_annotation() {
        let pdf = build_pdf_with_annots(&[(
            4,
            "<< /Type /Annot /Subtype /Highlight /Rect [10 20 200 40] /C [1 1 0] >>",
        )]);
        let doc = Document::parse(&pdf).unwrap();
        let annots = doc.annotations(0).unwrap();

        assert_eq!(annots[0].annot_type, AnnotationType::Highlight);
        assert_eq!(annots[0].color.as_deref(), Some(&[1.0, 1.0, 0.0][..]));
    }

    #[test]
    fn av7_annotation_with_flags() {
        let pdf = build_pdf_with_annots(&[(
            4,
            "<< /Type /Annot /Subtype /Text /Rect [0 0 10 10] /F 4 >>",
        )]);
        let doc = Document::parse(&pdf).unwrap();
        let annots = doc.annotations(0).unwrap();

        assert_eq!(annots[0].flags, 4); // NoZoom flag
    }

    #[test]
    fn av7_annotation_with_border() {
        let pdf = build_pdf_with_annots(&[(
            4,
            "<< /Type /Annot /Subtype /Link /Rect [0 0 100 20] /Border [0 0 1] >>",
        )]);
        let doc = Document::parse(&pdf).unwrap();
        let annots = doc.annotations(0).unwrap();

        assert_eq!(annots[0].border, Some([0.0, 0.0, 1.0]));
    }

    #[test]
    fn av7_annotation_with_appearance() {
        let pdf = build_pdf_with_annots(&[
            (
                4,
                "<< /Type /Annot /Subtype /Stamp /Rect [0 0 100 100] /AP << /N 5 0 R >> >>",
            ),
            (5, "<< /Type /XObject /Subtype /Form >>"),
        ]);
        let doc = Document::parse(&pdf).unwrap();
        let annots = doc.annotations(0).unwrap();

        assert!(annots[0].has_appearance);
    }

    #[test]
    fn av7_annotation_with_name_and_modified() {
        let pdf = build_pdf_with_annots(&[(
            4,
            "<< /Type /Annot /Subtype /Text /Rect [0 0 10 10] /NM (annot1) /M (D:20240101) >>",
        )]);
        let doc = Document::parse(&pdf).unwrap();
        let annots = doc.annotations(0).unwrap();

        assert_eq!(annots[0].name.as_deref(), Some("annot1"));
        assert_eq!(annots[0].modified.as_deref(), Some("D:20240101"));
    }

    #[test]
    fn av7_multiple_annotations() {
        let pdf = build_pdf_with_annots(&[
            (4, "<< /Type /Annot /Subtype /Text /Rect [0 0 10 10] >>"),
            (5, "<< /Type /Annot /Subtype /Link /Rect [20 20 100 40] >>"),
            (
                6,
                "<< /Type /Annot /Subtype /Highlight /Rect [0 50 200 60] >>",
            ),
        ]);
        let doc = Document::parse(&pdf).unwrap();
        let annots = doc.annotations(0).unwrap();

        assert_eq!(annots.len(), 3);
        assert_eq!(annots[0].annot_type, AnnotationType::Text);
        assert_eq!(annots[1].annot_type, AnnotationType::Link);
        assert_eq!(annots[2].annot_type, AnnotationType::Highlight);
    }

    #[test]
    fn av7_all_annotations() {
        let pdf =
            build_pdf_with_annots(&[(4, "<< /Type /Annot /Subtype /Text /Rect [0 0 10 10] >>")]);
        let doc = Document::parse(&pdf).unwrap();
        let all = doc.all_annotations().unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn av7_annotation_type_names() {
        assert_eq!(AnnotationType::Text.name(), "Text");
        assert_eq!(AnnotationType::Link.name(), "Link");
        assert_eq!(AnnotationType::Highlight.name(), "Highlight");
        assert_eq!(AnnotationType::ThreeD.name(), "3D");
        assert_eq!(AnnotationType::Unknown.name(), "Unknown");
    }

    #[test]
    fn av7_annotation_type_from_name() {
        assert_eq!(AnnotationType::from_name("Text"), AnnotationType::Text);
        assert_eq!(AnnotationType::from_name("3D"), AnnotationType::ThreeD);
        assert_eq!(AnnotationType::from_name("Bogus"), AnnotationType::Unknown);
    }

    #[test]
    fn av7_page_out_of_range() {
        let pdf = build_pdf_with_annots(&[]);
        let doc = Document::parse(&pdf).unwrap();
        assert!(doc.annotations(99).is_err());
    }

    // ====================================================================
    // AV6: Form field tests
    // ====================================================================

    #[test]
    fn av6_no_acroform() {
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
        assert!(doc.acro_form().unwrap().is_none());
    }

    #[test]
    fn av6_empty_form() {
        let pdf = build_pdf_with_form("/Fields []", &[]);
        let doc = Document::parse(&pdf).unwrap();
        let form = doc.acro_form().unwrap().unwrap();
        assert!(form.fields.is_empty());
        assert!(!form.need_appearances);
        assert_eq!(form.sig_flags, 0);
    }

    #[test]
    fn av6_text_field() {
        let pdf = build_pdf_with_form(
            "/Fields [4 0 R]",
            &[(
                4,
                "<< /FT /Tx /T (name) /V (John Doe) /Rect [100 700 300 720] >>",
            )],
        );
        let doc = Document::parse(&pdf).unwrap();
        let form = doc.acro_form().unwrap().unwrap();

        assert_eq!(form.fields.len(), 1);
        let field = &form.fields[0];
        assert_eq!(field.field_type, FieldType::Text);
        assert_eq!(field.partial_name.as_deref(), Some("name"));
        assert_eq!(field.full_name, "name");
        assert_eq!(field.value.as_deref(), Some("John Doe"));
        assert_eq!(field.rect, Some([100.0, 700.0, 300.0, 720.0]));
    }

    #[test]
    fn av6_button_field() {
        let pdf = build_pdf_with_form(
            "/Fields [4 0 R]",
            &[(4, "<< /FT /Btn /T (agree) /V /Yes >>")],
        );
        let doc = Document::parse(&pdf).unwrap();
        let form = doc.acro_form().unwrap().unwrap();

        assert_eq!(form.fields[0].field_type, FieldType::Button);
        assert_eq!(form.fields[0].value.as_deref(), Some("Yes"));
    }

    #[test]
    fn av6_choice_field_with_options() {
        let pdf = build_pdf_with_form(
            "/Fields [4 0 R]",
            &[(
                4,
                "<< /FT /Ch /T (color) /V (Red) /Opt [(Red) (Green) (Blue)] >>",
            )],
        );
        let doc = Document::parse(&pdf).unwrap();
        let form = doc.acro_form().unwrap().unwrap();

        let field = &form.fields[0];
        assert_eq!(field.field_type, FieldType::Choice);
        assert_eq!(field.value.as_deref(), Some("Red"));
        assert_eq!(field.options, vec!["Red", "Green", "Blue"]);
    }

    #[test]
    fn av6_signature_field() {
        let pdf = build_pdf_with_form(
            "/Fields [4 0 R] /SigFlags 3",
            &[(4, "<< /FT /Sig /T (sig1) >>")],
        );
        let doc = Document::parse(&pdf).unwrap();
        let form = doc.acro_form().unwrap().unwrap();

        assert_eq!(form.fields[0].field_type, FieldType::Signature);
        assert_eq!(form.sig_flags, 3);
    }

    #[test]
    fn av6_field_flags() {
        let pdf = build_pdf_with_form(
            "/Fields [4 0 R]",
            &[(4, "<< /FT /Tx /T (readonly) /Ff 1 >>")],
        );
        let doc = Document::parse(&pdf).unwrap();
        let form = doc.acro_form().unwrap().unwrap();

        assert!(form.fields[0].is_read_only());
        assert!(!form.fields[0].is_required());
    }

    #[test]
    fn av6_required_field() {
        let pdf = build_pdf_with_form(
            "/Fields [4 0 R]",
            &[(4, "<< /FT /Tx /T (required) /Ff 2 >>")],
        );
        let doc = Document::parse(&pdf).unwrap();
        let form = doc.acro_form().unwrap().unwrap();

        assert!(form.fields[0].is_required());
    }

    #[test]
    fn av6_hierarchical_fields() {
        let pdf = build_pdf_with_form(
            "/Fields [4 0 R]",
            &[
                (4, "<< /T (person) /Kids [5 0 R 6 0 R] >>"),
                (5, "<< /FT /Tx /T (first) /V (Jane) >>"),
                (6, "<< /FT /Tx /T (last) /V (Doe) >>"),
            ],
        );
        let doc = Document::parse(&pdf).unwrap();
        let form = doc.acro_form().unwrap().unwrap();

        assert_eq!(form.fields.len(), 1);
        let parent = &form.fields[0];
        assert_eq!(parent.partial_name.as_deref(), Some("person"));
        assert_eq!(parent.children.len(), 2);
        assert_eq!(parent.children[0].full_name, "person.first");
        assert_eq!(parent.children[0].value.as_deref(), Some("Jane"));
        assert_eq!(parent.children[1].full_name, "person.last");
        assert_eq!(parent.children[1].value.as_deref(), Some("Doe"));
    }

    #[test]
    fn av6_need_appearances() {
        let pdf = build_pdf_with_form("/Fields [] /NeedAppearances true", &[]);
        let doc = Document::parse(&pdf).unwrap();
        let form = doc.acro_form().unwrap().unwrap();
        assert!(form.need_appearances);
    }

    #[test]
    fn av6_total_field_count() {
        let pdf = build_pdf_with_form(
            "/Fields [4 0 R 7 0 R]",
            &[
                (4, "<< /T (parent) /Kids [5 0 R 6 0 R] >>"),
                (5, "<< /FT /Tx /T (a) >>"),
                (6, "<< /FT /Tx /T (b) >>"),
                (7, "<< /FT /Tx /T (standalone) >>"),
            ],
        );
        let doc = Document::parse(&pdf).unwrap();
        let form = doc.acro_form().unwrap().unwrap();

        // parent(1) + a(1) + b(1) + standalone(1) = 4
        assert_eq!(form.total_field_count(), 4);
    }

    #[test]
    fn av6_default_value() {
        let pdf = build_pdf_with_form(
            "/Fields [4 0 R]",
            &[(4, "<< /FT /Tx /T (f) /V (current) /DV (default) >>")],
        );
        let doc = Document::parse(&pdf).unwrap();
        let form = doc.acro_form().unwrap().unwrap();

        assert_eq!(form.fields[0].value.as_deref(), Some("current"));
        assert_eq!(form.fields[0].default_value.as_deref(), Some("default"));
    }

    #[test]
    fn av6_field_type_names() {
        assert_eq!(FieldType::Text.name(), "Text");
        assert_eq!(FieldType::Button.name(), "Button");
        assert_eq!(FieldType::Choice.name(), "Choice");
        assert_eq!(FieldType::Signature.name(), "Signature");
        assert_eq!(FieldType::Unknown.name(), "Unknown");
    }

    #[test]
    fn av6_multiple_top_level_fields() {
        let pdf = build_pdf_with_form(
            "/Fields [4 0 R 5 0 R 6 0 R]",
            &[
                (4, "<< /FT /Tx /T (field1) /V (a) >>"),
                (5, "<< /FT /Tx /T (field2) /V (b) >>"),
                (6, "<< /FT /Btn /T (field3) >>"),
            ],
        );
        let doc = Document::parse(&pdf).unwrap();
        let form = doc.acro_form().unwrap().unwrap();

        assert_eq!(form.fields.len(), 3);
        assert_eq!(form.fields[0].full_name, "field1");
        assert_eq!(form.fields[1].full_name, "field2");
        assert_eq!(form.fields[2].full_name, "field3");
    }
}
