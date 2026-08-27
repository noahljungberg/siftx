use napi::bindgen_prelude::*;
use napi_derive::napi;

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

/// Map siftx::core::Error to napi::Error with prefixed messages for JS-side classification.
fn map_err(e: siftx::core::Error) -> napi::Error {
    let msg = e.to_string();
    match e {
        siftx::core::Error::Io(_) => {
            napi::Error::new(Status::GenericFailure, format!("SiftIOError: {msg}"))
        }
        siftx::core::Error::Format(_) | siftx::core::Error::Truncated { .. } => {
            napi::Error::new(Status::GenericFailure, format!("SiftFormatError: {msg}"))
        }
        siftx::core::Error::Unsupported(_) => {
            napi::Error::new(Status::GenericFailure, format!("SiftError: unsupported: {msg}"))
        }
        _ => napi::Error::new(Status::GenericFailure, format!("SiftError: {msg}")),
    }
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[napi]
pub enum FileType {
    Unknown = 0,
    Jpeg = 1,
    Png = 2,
    Gif = 3,
    Bmp = 4,
    Tiff = 5,
    WebP = 6,
    Heif = 7,
    Pdf = 8,
    Icc = 9,
    QuickTime = 10,
}

fn to_js_file_type(ft: Option<siftx::core::FileType>) -> FileType {
    match ft {
        Some(siftx::core::FileType::Jpeg) => FileType::Jpeg,
        Some(siftx::core::FileType::Png) => FileType::Png,
        Some(siftx::core::FileType::Gif) => FileType::Gif,
        Some(siftx::core::FileType::Bmp) => FileType::Bmp,
        Some(siftx::core::FileType::Tiff) => FileType::Tiff,
        Some(siftx::core::FileType::WebP) => FileType::WebP,
        Some(siftx::core::FileType::Heif) => FileType::Heif,
        Some(siftx::core::FileType::Pdf) => FileType::Pdf,
        Some(siftx::core::FileType::Icc) => FileType::Icc,
        Some(siftx::core::FileType::QuickTime) => FileType::QuickTime,
        _ => FileType::Unknown,
    }
}

#[napi]
pub enum ImageFormat {
    Jpeg = 0,
    Jpeg2000 = 1,
    Jbig2 = 2,
    Ccitt = 3,
    Pixels = 4,
}

// ---------------------------------------------------------------------------
// Data objects
// ---------------------------------------------------------------------------

#[napi(object)]
pub struct Tag {
    pub group: String,
    pub name: String,
    pub value: String,
}

#[napi(object)]
pub struct GpsCoordinates {
    pub latitude: f64,
    pub longitude: f64,
    /// Meters above sea level, or null if unavailable.
    pub altitude: Option<f64>,
}

#[napi(object)]
pub struct ExtractedImage {
    pub page: u32,
    pub width: u32,
    pub height: u32,
    pub bits_per_component: u8,
    pub components: u8,
    pub format: ImageFormat,
    pub data: Buffer,
    pub extension: String,
    pub is_passthrough: bool,
}

#[napi(object)]
pub struct FormField {
    pub field_type: String,
    pub name: String,
    pub value: Option<String>,
    pub default_value: Option<String>,
    pub flags: u32,
    pub is_read_only: bool,
    pub is_required: bool,
    pub options: Vec<String>,
    pub children: Vec<FormField>,
}

#[napi(object)]
pub struct AcroFormData {
    pub fields: Vec<FormField>,
    pub need_appearances: bool,
    pub sig_flags: u32,
    pub total_field_count: u32,
}

#[napi(object)]
pub struct Annotation {
    pub annot_type: String,
    pub page: u32,
    pub rect: Vec<f64>,
    pub contents: Option<String>,
    pub dest: Option<String>,
    pub name: Option<String>,
    pub modified: Option<String>,
    pub flags: u32,
    pub has_appearance: bool,
}

#[napi(object)]
pub struct StructElementData {
    pub struct_type: String,
    pub title: Option<String>,
    pub alt_text: Option<String>,
    pub actual_text: Option<String>,
    pub lang: Option<String>,
    pub children: Vec<StructChildData>,
}

#[napi(object)]
pub struct StructChildData {
    /// "element", "content_ref", or "object_ref"
    pub child_type: String,
    pub element: Option<StructElementData>,
    pub mcid: Option<u32>,
    pub obj_num: Option<u32>,
}

#[napi(object)]
pub struct StructTreeData {
    pub root_elements: Vec<StructElementData>,
    pub role_map: Vec<RoleMapEntry>,
}

#[napi(object)]
pub struct RoleMapEntry {
    pub custom: String,
    pub standard: String,
}

// ---------------------------------------------------------------------------
// Helper conversion functions
// ---------------------------------------------------------------------------

fn convert_form_field(f: &siftx::FormField) -> FormField {
    FormField {
        field_type: f.field_type.name().to_string(),
        name: f.full_name.clone(),
        value: f.value.clone(),
        default_value: f.default_value.clone(),
        flags: f.flags,
        is_read_only: f.is_read_only(),
        is_required: f.is_required(),
        options: f.options.clone(),
        children: f.children.iter().map(convert_form_field).collect(),
    }
}

fn convert_struct_element(e: &siftx::StructElement) -> StructElementData {
    StructElementData {
        struct_type: e.struct_type.clone(),
        title: e.title.clone(),
        alt_text: e.alt_text.clone(),
        actual_text: e.actual_text.clone(),
        lang: e.lang.clone(),
        children: e.children.iter().map(convert_struct_child).collect(),
    }
}

fn convert_struct_child(c: &siftx::StructChild) -> StructChildData {
    match c {
        siftx::StructChild::Element(e) => StructChildData {
            child_type: "element".to_string(),
            element: Some(convert_struct_element(e)),
            mcid: None,
            obj_num: None,
        },
        siftx::StructChild::ContentRef(r) => StructChildData {
            child_type: "content_ref".to_string(),
            element: None,
            mcid: Some(r.mcid),
            obj_num: None,
        },
        siftx::StructChild::ObjectRef(n) => StructChildData {
            child_type: "object_ref".to_string(),
            element: None,
            mcid: None,
            obj_num: Some(*n),
        },
    }
}

// ---------------------------------------------------------------------------
// Owned document wrapper (handles lifetime erasure)
// ---------------------------------------------------------------------------

/// Wrapper that owns data and the parsed document.
///
/// `SiftDocument<'a>` borrows from data. For napi we need 'static,
/// so we own the data and transmute the lifetime (same pattern as FFI layer).
struct OwnedDocument {
    _data: OwnedData,
    inner: siftx::SiftDocument<'static>,
}

/// Both variants are write-only on purpose: they exist solely to keep the
/// backing memory alive for as long as the transmuted `'static` document
/// borrows from it. Dropping either field would be a use-after-free.
#[allow(dead_code)]
enum OwnedData {
    File(siftx::SiftFile),
    Buffer(Vec<u8>),
}

impl OwnedDocument {
    fn from_file(file: siftx::SiftFile) -> Result<Self> {
        // SAFETY: We store the SiftFile in _data, keeping the mmap alive.
        // The document borrows from file.data() which lives as long as OwnedData::File.
        let doc = file.parse().map_err(map_err)?;
        let doc: siftx::SiftDocument<'static> = unsafe { std::mem::transmute(doc) };
        Ok(Self {
            _data: OwnedData::File(file),
            inner: doc,
        })
    }

    fn from_buffer(data: Vec<u8>) -> Result<Self> {
        // SAFETY: We store the Vec in _data, keeping the bytes alive.
        let borrowed: &[u8] = &data;
        let borrowed_static: &'static [u8] = unsafe { std::mem::transmute(borrowed) };
        let doc = siftx::read(borrowed_static).map_err(map_err)?;
        Ok(Self {
            _data: OwnedData::Buffer(data),
            inner: doc,
        })
    }
}

// ---------------------------------------------------------------------------
// SiftFile class
// ---------------------------------------------------------------------------

#[napi]
pub struct SiftFile {
    file: Option<siftx::SiftFile>,
}

#[napi]
impl SiftFile {
    /// Open a file by path (memory-mapped).
    #[napi(factory)]
    pub fn open(path: String) -> Result<SiftFile> {
        let file = siftx::open(&path).map_err(map_err)?;
        Ok(SiftFile { file: Some(file) })
    }

    /// Detected file type.
    #[napi(getter)]
    pub fn file_type(&self) -> Result<FileType> {
        let file = self.file.as_ref().ok_or_else(|| {
            napi::Error::new(Status::GenericFailure, "SiftFile is closed")
        })?;
        Ok(to_js_file_type(file.file_type()))
    }

    /// Parse into a SiftDocument.
    #[napi]
    pub fn parse(&mut self) -> Result<SiftDocument> {
        let file = self.file.take().ok_or_else(|| {
            napi::Error::new(Status::GenericFailure, "SiftFile is closed")
        })?;
        let owned = OwnedDocument::from_file(file)?;
        Ok(SiftDocument {
            inner: Some(owned),
        })
    }

    /// Release the memory mapping.
    #[napi]
    pub fn close(&mut self) {
        self.file = None;
    }
}

// ---------------------------------------------------------------------------
// SiftDocument class
// ---------------------------------------------------------------------------

#[napi]
pub struct SiftDocument {
    inner: Option<OwnedDocument>,
}

#[napi]
impl SiftDocument {
    fn doc(&self) -> Result<&siftx::SiftDocument<'static>> {
        self.inner
            .as_ref()
            .map(|o| &o.inner)
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "SiftDocument is closed"))
    }

    /// Detected file type.
    #[napi(getter)]
    pub fn file_type(&self) -> Result<FileType> {
        Ok(to_js_file_type(self.doc()?.file_type()))
    }

    /// All metadata tags.
    #[napi]
    pub fn tags(&self) -> Result<Vec<Tag>> {
        let tags = self.doc()?.tags();
        Ok(tags
            .into_iter()
            .map(|t| Tag {
                group: t.group.to_string(),
                name: t.name,
                value: t.value,
            })
            .collect())
    }

    /// GPS coordinates, or null if not present.
    #[napi]
    pub fn gps(&self) -> Result<Option<GpsCoordinates>> {
        let gps = self.doc()?.gps();
        Ok(gps.map(|g| GpsCoordinates {
            latitude: g.latitude,
            longitude: g.longitude,
            altitude: g.altitude,
        }))
    }

    /// EXIF thumbnail bytes (JPEG), or null.
    #[napi]
    pub fn thumbnail(&self) -> Result<Option<Buffer>> {
        let thumb = self.doc()?.thumbnail();
        Ok(thumb.map(Buffer::from))
    }

    /// Extracted PDF images.
    #[napi]
    pub fn images(&self) -> Result<Vec<ExtractedImage>> {
        let images = self.doc()?.images().map_err(map_err)?;
        Ok(images
            .into_iter()
            .map(|img| {
                let ext = img.extension().to_string();
                let is_passthrough =
                    matches!(&img.data, siftx::ImageData::Jpeg(_) | siftx::ImageData::Jpeg2000(_));
                let (format, data) = match img.data {
                    siftx::ImageData::Jpeg(d) => (ImageFormat::Jpeg, d),
                    siftx::ImageData::Jpeg2000(d) => (ImageFormat::Jpeg2000, d),
                    siftx::ImageData::Jbig2 { data: d, .. } => (ImageFormat::Jbig2, d),
                    siftx::ImageData::Ccitt(d) => (ImageFormat::Ccitt, d),
                    siftx::ImageData::Pixels(d) => (ImageFormat::Pixels, d),
                };
                ExtractedImage {
                    page: img.page,
                    width: img.width,
                    height: img.height,
                    bits_per_component: img.bpc,
                    components: img.components,
                    format,
                    data: Buffer::from(data),
                    extension: ext,
                    is_passthrough,
                }
            })
            .collect())
    }

    /// Text per PDF page.
    #[napi]
    pub fn text_pages(&self) -> Result<Vec<String>> {
        self.doc()?.text_pages().map_err(map_err)
    }

    /// EXIF tags only.
    #[napi]
    pub fn exif_tags(&self) -> Result<Vec<Tag>> {
        let tags = self.doc()?.exif_tags();
        Ok(tags.into_iter().map(|t| Tag { group: t.group.to_string(), name: t.name, value: t.value }).collect())
    }

    /// XMP tags only.
    #[napi]
    pub fn xmp_tags(&self) -> Result<Vec<Tag>> {
        let tags = self.doc()?.xmp_tags();
        Ok(tags.into_iter().map(|t| Tag { group: t.group.to_string(), name: t.name, value: t.value }).collect())
    }

    /// IPTC tags only.
    #[napi]
    pub fn iptc_tags(&self) -> Result<Vec<Tag>> {
        let tags = self.doc()?.iptc_tags();
        Ok(tags.into_iter().map(|t| Tag { group: t.group.to_string(), name: t.name, value: t.value }).collect())
    }

    /// Raw text per PDF page (no layout).
    #[napi]
    pub fn text_pages_raw(&self) -> Result<Vec<String>> {
        self.doc()?.text_pages_raw().map_err(map_err)
    }

    /// Authenticate encrypted PDF. Returns true if password accepted.
    #[napi]
    pub fn authenticate(&mut self, password: Buffer) -> Result<bool> {
        let doc = self.inner.as_mut()
            .map(|o| &mut o.inner)
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "SiftDocument is closed"))?;
        Ok(doc.authenticate(&password))
    }

    /// PDF form fields, or null if no form.
    #[napi]
    pub fn acro_form(&self) -> Result<Option<AcroFormData>> {
        let form = self.doc()?.acro_form().map_err(map_err)?;
        Ok(form.map(|f| {
            let total = f.total_field_count();
            AcroFormData {
                fields: f.fields.iter().map(convert_form_field).collect(),
                need_appearances: f.need_appearances,
                sig_flags: f.sig_flags,
                total_field_count: total as u32,
            }
        }))
    }

    /// All PDF annotations.
    #[napi]
    pub fn annotations(&self) -> Result<Vec<Annotation>> {
        let annots = self.doc()?.all_annotations().map_err(map_err)?;
        Ok(annots.into_iter().map(|a| Annotation {
            annot_type: a.annot_type.name().to_string(),
            page: a.page_index as u32,
            rect: a.rect.to_vec(),
            contents: a.contents,
            dest: a.dest,
            name: a.name,
            modified: a.modified,
            flags: a.flags,
            has_appearance: a.has_appearance,
        }).collect())
    }

    /// Tagged PDF structure tree, or null if not tagged.
    #[napi]
    pub fn struct_tree(&self) -> Result<Option<StructTreeData>> {
        let tree = self.doc()?.struct_tree().map_err(map_err)?;
        Ok(tree.map(|t| StructTreeData {
            root_elements: t.root_elements.iter().map(convert_struct_element).collect(),
            role_map: t.role_map.into_iter().map(|(k, v)| RoleMapEntry { custom: k, standard: v }).collect(),
        }))
    }

    /// Release native resources.
    #[napi]
    pub fn close(&mut self) {
        self.inner = None;
    }
}

// ---------------------------------------------------------------------------
// Convenience functions
// ---------------------------------------------------------------------------

/// Extract tags from a file path in one call.
#[napi]
pub fn tags(path: String) -> Result<Vec<Tag>> {
    let rust_tags = siftx::tags(&path).map_err(map_err)?;
    Ok(rust_tags
        .into_iter()
        .map(|t| Tag {
            group: t.group.to_string(),
            name: t.name,
            value: t.value,
        })
        .collect())
}

/// Parse a document from a Buffer.
#[napi]
pub fn read(data: Buffer) -> Result<SiftDocument> {
    let owned = OwnedDocument::from_buffer(data.to_vec())?;
    Ok(SiftDocument {
        inner: Some(owned),
    })
}

/// Native library version.
#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
