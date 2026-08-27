//! PDF tagged structure tree traversal (AV5).
//!
//! Parses the structure tree root (/StructTreeRoot) for tagged PDFs,
//! extracting the document's logical structure: headings, paragraphs,
//! tables, figures, etc.
//! Per ISO 32000-2 §14.7.

use super::document::Document;
use super::object::PdfObject;
use crate::core::Result;

/// A complete structure tree from a tagged PDF.
#[derive(Debug, Clone)]
pub struct StructTree {
    /// Root-level structure elements.
    pub root_elements: Vec<StructElement>,
    /// Role map: maps custom role names to standard structure types.
    pub role_map: Vec<(String, String)>,
}

/// A single node in the structure tree.
#[derive(Debug, Clone)]
pub struct StructElement {
    /// Structure type (e.g., "Document", "P", "H1", "Table", "Figure", "Span").
    pub struct_type: String,
    /// Children: sub-elements or content references.
    pub children: Vec<StructChild>,
    /// /Alt - alternative text for accessibility.
    pub alt_text: Option<String>,
    /// /ActualText - replacement text.
    pub actual_text: Option<String>,
    /// /Lang - language tag (e.g., "en-US").
    pub lang: Option<String>,
    /// /T - title.
    pub title: Option<String>,
}

/// A child of a structure element.
#[derive(Debug, Clone)]
pub enum StructChild {
    /// A nested structure element.
    Element(StructElement),
    /// A marked content reference (MCID on a specific page).
    ContentRef(McidRef),
    /// An object reference (e.g., annotation or form field).
    ObjectRef(u32),
}

/// A marked content identifier reference.
#[derive(Debug, Clone)]
pub struct McidRef {
    /// The page object number this MCID belongs to.
    pub page_obj_num: Option<u32>,
    /// The marked content identifier.
    pub mcid: u32,
}

impl<'a> Document<'a> {
    /// AV5: Parse the structure tree from the document catalog.
    ///
    /// Returns `None` if the document is not tagged (no /StructTreeRoot).
    pub fn struct_tree(&self) -> Result<Option<StructTree>> {
        let catalog = self.catalog()?;

        let tree_root_ref = match catalog.dict_get(b"StructTreeRoot") {
            Some(r) => r,
            None => return Ok(None),
        };

        let tree_root = self.resolve_obj(tree_root_ref)?;

        // Parse /RoleMap
        let role_map = self.parse_role_map(&tree_root)?;

        // Parse /K (children of the structure tree root)
        let root_elements = self.parse_struct_children(&tree_root, 0)?;

        Ok(Some(StructTree {
            root_elements,
            role_map,
        }))
    }

    /// Parse the /RoleMap dictionary.
    fn parse_role_map(&self, tree_root: &PdfObject) -> Result<Vec<(String, String)>> {
        let mut map = Vec::new();

        if let Some(role_map_ref) = tree_root.dict_get(b"RoleMap") {
            let role_map = self.resolve_obj(role_map_ref)?;
            if let Some(entries) = role_map.as_dict() {
                for (key, value) in entries {
                    let custom = String::from_utf8_lossy(key).to_string();
                    if let Some(standard) = value.as_name_str() {
                        map.push((custom, standard.to_string()));
                    }
                }
            }
        }

        Ok(map)
    }

    /// Parse the children (/K) of a structure element.
    fn parse_struct_children(&self, element: &PdfObject, depth: u32) -> Result<Vec<StructElement>> {
        // Prevent infinite recursion
        if depth > 100 {
            return Ok(Vec::new());
        }

        let k = match element.dict_get(b"K") {
            Some(k) => k.clone(),
            None => return Ok(Vec::new()),
        };

        // /K can be a single item or an array
        let items = match &k {
            PdfObject::Array(arr) => arr.clone(),
            other => vec![other.clone()],
        };

        let mut elements = Vec::new();

        for item in &items {
            match item {
                PdfObject::Ref(_) => {
                    let resolved = self.resolve_obj(item)?;
                    if let Some(elem) = self.parse_single_struct_element(&resolved, depth)? {
                        elements.push(elem);
                    }
                }
                PdfObject::Dict(_) => {
                    if let Some(elem) = self.parse_single_struct_element(item, depth)? {
                        elements.push(elem);
                    }
                }
                PdfObject::Int(_) => {
                    // Integer = MCID (marked content identifier)
                    // This is a leaf content reference, not a struct element
                }
                _ => {}
            }
        }

        Ok(elements)
    }

    /// Parse a single structure element from a dictionary.
    fn parse_single_struct_element(
        &self,
        obj: &PdfObject,
        depth: u32,
    ) -> Result<Option<StructElement>> {
        // Must have /S (structure type)
        let struct_type = match obj.dict_get(b"S") {
            Some(s) => match s.as_name_str() {
                Some(name) => name.to_string(),
                None => return Ok(None),
            },
            None => return Ok(None), // Not a structure element (could be MCID dict)
        };

        let alt_text = obj
            .dict_get(b"Alt")
            .and_then(|a| a.as_string())
            .map(|s| String::from_utf8_lossy(s).to_string());

        let actual_text = obj
            .dict_get(b"ActualText")
            .and_then(|a| a.as_string())
            .map(|s| String::from_utf8_lossy(s).to_string());

        let lang = obj
            .dict_get(b"Lang")
            .and_then(|l| l.as_string())
            .map(|s| String::from_utf8_lossy(s).to_string());

        let title = obj
            .dict_get(b"T")
            .and_then(|t| t.as_string())
            .map(|s| String::from_utf8_lossy(s).to_string());

        // Parse children from /K
        let children = self.parse_k_children(obj, depth + 1)?;

        Ok(Some(StructElement {
            struct_type,
            children,
            alt_text,
            actual_text,
            lang,
            title,
        }))
    }

    /// Parse /K children into StructChild items.
    fn parse_k_children(&self, element: &PdfObject, depth: u32) -> Result<Vec<StructChild>> {
        if depth > 100 {
            return Ok(Vec::new());
        }

        let k = match element.dict_get(b"K") {
            Some(k) => k.clone(),
            None => return Ok(Vec::new()),
        };

        let items = match &k {
            PdfObject::Array(arr) => arr.clone(),
            other => vec![other.clone()],
        };

        let mut children = Vec::new();

        for item in &items {
            match item {
                PdfObject::Int(mcid) => {
                    // Direct MCID integer
                    children.push(StructChild::ContentRef(McidRef {
                        page_obj_num: None,
                        mcid: *mcid as u32,
                    }));
                }
                PdfObject::Ref(_) => {
                    let resolved = self.resolve_obj(item)?;
                    self.classify_k_item(&resolved, depth, &mut children)?;
                }
                PdfObject::Dict(_) => {
                    self.classify_k_item(item, depth, &mut children)?;
                }
                _ => {}
            }
        }

        Ok(children)
    }

    /// Classify a resolved /K item: is it an MCID dict, an OBJR, or a struct element?
    fn classify_k_item(
        &self,
        item: &PdfObject,
        depth: u32,
        children: &mut Vec<StructChild>,
    ) -> Result<()> {
        // Check /Type
        let item_type = item
            .dict_get(b"Type")
            .and_then(|t| t.as_name_str())
            .unwrap_or("");

        match item_type {
            "MCR" => {
                // Marked content reference dict: /Type /MCR /Pg ref /MCID int
                let mcid = item.dict_get(b"MCID").and_then(|m| m.as_int()).unwrap_or(0) as u32;
                let page_obj_num = item.dict_get(b"Pg").and_then(|p| p.as_ref()).map(|r| r.num);
                children.push(StructChild::ContentRef(McidRef { page_obj_num, mcid }));
            }
            "OBJR" => {
                // Object reference: /Type /OBJR /Obj ref
                let obj_num = item
                    .dict_get(b"Obj")
                    .and_then(|o| o.as_ref())
                    .map(|r| r.num)
                    .unwrap_or(0);
                children.push(StructChild::ObjectRef(obj_num));
            }
            _ => {
                // Check if it's a structure element (has /S)
                if item.dict_get(b"S").is_some() {
                    if let Some(elem) = self.parse_single_struct_element(item, depth)? {
                        children.push(StructChild::Element(elem));
                    }
                } else if let Some(mcid) = item.dict_get(b"MCID").and_then(|m| m.as_int()) {
                    // Untyped MCID dict
                    let page_obj_num = item.dict_get(b"Pg").and_then(|p| p.as_ref()).map(|r| r.num);
                    children.push(StructChild::ContentRef(McidRef {
                        page_obj_num,
                        mcid: mcid as u32,
                    }));
                }
            }
        }

        Ok(())
    }
}

impl StructTree {
    /// Total number of structure elements (recursive count).
    pub fn element_count(&self) -> usize {
        self.root_elements.iter().map(|e| e.element_count()).sum()
    }

    /// Lookup a role mapping: custom -> standard.
    pub fn map_role(&self, custom: &str) -> Option<&str> {
        self.role_map
            .iter()
            .find(|(k, _)| k == custom)
            .map(|(_, v)| v.as_str())
    }
}

impl StructElement {
    /// Recursive count of this element and all descendants.
    pub fn element_count(&self) -> usize {
        1 + self
            .children
            .iter()
            .map(|c| match c {
                StructChild::Element(e) => e.element_count(),
                _ => 0,
            })
            .sum::<usize>()
    }

    /// Get the effective structure type, resolving via role map.
    pub fn effective_type<'a>(&'a self, tree: &'a StructTree) -> &'a str {
        tree.map_role(&self.struct_type)
            .unwrap_or(&self.struct_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a PDF with a structure tree.
    fn build_tagged_pdf(struct_tree_entries: &str, extra_objects: &[(u32, &str)]) -> Vec<u8> {
        let mut pdf = b"%PDF-1.7\n".to_vec();
        let mut offsets: Vec<(u32, usize)> = Vec::new();

        // Object 1: Catalog
        offsets.push((1, pdf.len()));
        pdf.extend_from_slice(
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 3 0 R /MarkInfo << /Marked true >> >>\nendobj\n"
        );

        // Object 2: Pages
        offsets.push((2, pdf.len()));
        pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [4 0 R] /Count 1 >>\nendobj\n");

        // Object 3: StructTreeRoot
        offsets.push((3, pdf.len()));
        pdf.extend_from_slice(
            format!(
                "3 0 obj\n<< /Type /StructTreeRoot {} >>\nendobj\n",
                struct_tree_entries
            )
            .as_bytes(),
        );

        // Object 4: Page
        offsets.push((4, pdf.len()));
        pdf.extend_from_slice(
            b"4 0 obj\n<< /Type /Page /MediaBox [0 0 612 792] /Parent 2 0 R >>\nendobj\n",
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

    // --- AV5: Basic structure tree ---

    #[test]
    fn av5_no_struct_tree() {
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
        assert!(doc.struct_tree().unwrap().is_none());
    }

    #[test]
    fn av5_empty_struct_tree() {
        let pdf = build_tagged_pdf("", &[]);
        let doc = Document::parse(&pdf).unwrap();
        let tree = doc.struct_tree().unwrap().unwrap();
        assert!(tree.root_elements.is_empty());
        assert!(tree.role_map.is_empty());
    }

    #[test]
    fn av5_single_element() {
        let pdf = build_tagged_pdf("/K << /S /Document /K 0 >>", &[]);
        let doc = Document::parse(&pdf).unwrap();
        let tree = doc.struct_tree().unwrap().unwrap();

        assert_eq!(tree.root_elements.len(), 1);
        assert_eq!(tree.root_elements[0].struct_type, "Document");
    }

    #[test]
    fn av5_nested_elements() {
        let pdf = build_tagged_pdf(
            "/K << /S /Document /K [<< /S /H1 /K 0 >> << /S /P /K 1 >>] >>",
            &[],
        );
        let doc = Document::parse(&pdf).unwrap();
        let tree = doc.struct_tree().unwrap().unwrap();

        assert_eq!(tree.root_elements.len(), 1);
        let doc_elem = &tree.root_elements[0];
        assert_eq!(doc_elem.struct_type, "Document");

        // Should have 2 child elements (H1 and P) + their MCID content refs
        let child_elements: Vec<_> = doc_elem
            .children
            .iter()
            .filter_map(|c| match c {
                StructChild::Element(e) => Some(e),
                _ => None,
            })
            .collect();
        assert_eq!(child_elements.len(), 2);
        assert_eq!(child_elements[0].struct_type, "H1");
        assert_eq!(child_elements[1].struct_type, "P");
    }

    #[test]
    fn av5_element_with_alt_text() {
        let pdf = build_tagged_pdf("/K << /S /Figure /Alt (A photo of a sunset) /K 0 >>", &[]);
        let doc = Document::parse(&pdf).unwrap();
        let tree = doc.struct_tree().unwrap().unwrap();

        let fig = &tree.root_elements[0];
        assert_eq!(fig.struct_type, "Figure");
        assert_eq!(fig.alt_text.as_deref(), Some("A photo of a sunset"));
    }

    #[test]
    fn av5_element_with_actual_text() {
        let pdf = build_tagged_pdf("/K << /S /Span /ActualText (Hello) /K 0 >>", &[]);
        let doc = Document::parse(&pdf).unwrap();
        let tree = doc.struct_tree().unwrap().unwrap();

        assert_eq!(tree.root_elements[0].actual_text.as_deref(), Some("Hello"));
    }

    #[test]
    fn av5_element_with_lang() {
        let pdf = build_tagged_pdf("/K << /S /Document /Lang (en-US) >>", &[]);
        let doc = Document::parse(&pdf).unwrap();
        let tree = doc.struct_tree().unwrap().unwrap();

        assert_eq!(tree.root_elements[0].lang.as_deref(), Some("en-US"));
    }

    #[test]
    fn av5_element_with_title() {
        let pdf = build_tagged_pdf("/K << /S /Document /T (My Document) >>", &[]);
        let doc = Document::parse(&pdf).unwrap();
        let tree = doc.struct_tree().unwrap().unwrap();

        assert_eq!(tree.root_elements[0].title.as_deref(), Some("My Document"));
    }

    #[test]
    fn av5_role_map() {
        let pdf = build_tagged_pdf(
            "/RoleMap << /MyHeading /H1 /MyPara /P >> /K << /S /MyHeading /K 0 >>",
            &[],
        );
        let doc = Document::parse(&pdf).unwrap();
        let tree = doc.struct_tree().unwrap().unwrap();

        assert_eq!(tree.role_map.len(), 2);
        assert_eq!(tree.map_role("MyHeading"), Some("H1"));
        assert_eq!(tree.map_role("MyPara"), Some("P"));
        assert_eq!(tree.map_role("Unknown"), None);

        // Test effective_type
        let elem = &tree.root_elements[0];
        assert_eq!(elem.struct_type, "MyHeading");
        assert_eq!(elem.effective_type(&tree), "H1");
    }

    #[test]
    fn av5_mcid_content_ref() {
        let pdf = build_tagged_pdf("/K << /S /P /K [0 1 2] >>", &[]);
        let doc = Document::parse(&pdf).unwrap();
        let tree = doc.struct_tree().unwrap().unwrap();

        let p = &tree.root_elements[0];
        let content_refs: Vec<_> = p
            .children
            .iter()
            .filter_map(|c| match c {
                StructChild::ContentRef(r) => Some(r),
                _ => None,
            })
            .collect();
        assert_eq!(content_refs.len(), 3);
        assert_eq!(content_refs[0].mcid, 0);
        assert_eq!(content_refs[1].mcid, 1);
        assert_eq!(content_refs[2].mcid, 2);
    }

    #[test]
    fn av5_mcid_dict_ref() {
        let pdf = build_tagged_pdf("/K << /S /P /K << /Type /MCR /MCID 5 >> >>", &[]);
        let doc = Document::parse(&pdf).unwrap();
        let tree = doc.struct_tree().unwrap().unwrap();

        let p = &tree.root_elements[0];
        let content_refs: Vec<_> = p
            .children
            .iter()
            .filter_map(|c| match c {
                StructChild::ContentRef(r) => Some(r),
                _ => None,
            })
            .collect();
        assert_eq!(content_refs.len(), 1);
        assert_eq!(content_refs[0].mcid, 5);
    }

    #[test]
    fn av5_object_ref() {
        let pdf = build_tagged_pdf(
            "/K << /S /Link /K << /Type /OBJR /Obj 10 0 R >> >>",
            &[(10, "<< /Type /Annot /Subtype /Link >>")],
        );
        let doc = Document::parse(&pdf).unwrap();
        let tree = doc.struct_tree().unwrap().unwrap();

        let link = &tree.root_elements[0];
        let obj_refs: Vec<_> = link
            .children
            .iter()
            .filter_map(|c| match c {
                StructChild::ObjectRef(n) => Some(*n),
                _ => None,
            })
            .collect();
        assert_eq!(obj_refs, vec![10]);
    }

    #[test]
    fn av5_element_count() {
        let pdf = build_tagged_pdf(
            "/K << /S /Document /K [<< /S /H1 /K 0 >> << /S /P /K [<< /S /Span /K 1 >>] >>] >>",
            &[],
        );
        let doc = Document::parse(&pdf).unwrap();
        let tree = doc.struct_tree().unwrap().unwrap();

        // Document -> H1, P -> Span = 4 elements total
        assert_eq!(tree.element_count(), 4);
    }

    #[test]
    fn av5_indirect_children() {
        let pdf = build_tagged_pdf(
            "/K 5 0 R",
            &[(5, "<< /S /Document /K [<< /S /P /K 0 >>] >>")],
        );
        let doc = Document::parse(&pdf).unwrap();
        let tree = doc.struct_tree().unwrap().unwrap();

        assert_eq!(tree.root_elements.len(), 1);
        assert_eq!(tree.root_elements[0].struct_type, "Document");
    }

    #[test]
    fn av5_array_of_root_elements() {
        let pdf = build_tagged_pdf("/K [<< /S /H1 /K 0 >> << /S /P /K 1 >>]", &[]);
        let doc = Document::parse(&pdf).unwrap();
        let tree = doc.struct_tree().unwrap().unwrap();

        assert_eq!(tree.root_elements.len(), 2);
        assert_eq!(tree.root_elements[0].struct_type, "H1");
        assert_eq!(tree.root_elements[1].struct_type, "P");
    }
}
