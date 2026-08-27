use pyo3::prelude::*;
use pyo3::exceptions::{PyIOError, PyValueError, PyRuntimeError};
use pyo3::types::PyBytes;

// Aliased so `siftx` can name the Python module below.
use ::siftx as siftx_rs;
use siftx_rs::core::{Error as SiftCoreError, FileType as RustFileType};

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

fn map_err(e: SiftCoreError) -> PyErr {
    match e {
        SiftCoreError::Io(io) => PyIOError::new_err(format!("SiftIOError: {io}")),
        SiftCoreError::Format(msg) => SiftFormatError::new_err(format!("format error: {msg}")),
        SiftCoreError::Truncated { needed, available } => {
            SiftFormatError::new_err(format!(
                "truncated: need {needed} bytes, only {available} available"
            ))
        }
        SiftCoreError::Unsupported(msg) => {
            PyValueError::new_err(format!("unsupported: {msg}"))
        }
        SiftCoreError::Cycle(offset) => {
            SiftError::new_err(format!("cycle detected at offset {offset}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Exception classes
// ---------------------------------------------------------------------------

pyo3::create_exception!(
    siftx,
    SiftError,
    pyo3::exceptions::PyException,
    "Base class for every error siftx raises."
);
pyo3::create_exception!(
    siftx,
    SiftIOError,
    PyIOError,
    "A file could not be opened or read. Also an OSError."
);
pyo3::create_exception!(
    siftx,
    SiftFormatError,
    SiftError,
    "The data is not a format siftx understands, or is too damaged to parse."
);

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Detected file format of an opened file.
#[pyclass(eq, eq_int, skip_from_py_object)]
#[derive(Clone, Copy, PartialEq)]
enum FileType {
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

fn to_py_file_type(ft: Option<RustFileType>) -> FileType {
    match ft {
        Some(RustFileType::Jpeg) => FileType::Jpeg,
        Some(RustFileType::Png) => FileType::Png,
        Some(RustFileType::Gif) => FileType::Gif,
        Some(RustFileType::Bmp) => FileType::Bmp,
        Some(RustFileType::Tiff) => FileType::Tiff,
        Some(RustFileType::WebP) => FileType::WebP,
        Some(RustFileType::Heif) => FileType::Heif,
        Some(RustFileType::Pdf) => FileType::Pdf,
        Some(RustFileType::Icc) => FileType::Icc,
        Some(RustFileType::QuickTime) => FileType::QuickTime,
        _ => FileType::Unknown,
    }
}

/// Encoding of an image extracted from a PDF.
#[pyclass(eq, eq_int, skip_from_py_object)]
#[derive(Clone, Copy, PartialEq)]
enum ImageFormat {
    Jpeg = 0,
    Jpeg2000 = 1,
    Jbig2 = 2,
    Ccitt = 3,
    Pixels = 4,
}

// ---------------------------------------------------------------------------
// Data classes
// ---------------------------------------------------------------------------

/// One metadata tag: its group, its name, and a display-ready value.
#[pyclass(frozen, get_all, skip_from_py_object)]
#[derive(Clone)]
struct Tag {
    group: String,
    name: String,
    value: String,
}

#[pymethods]
impl Tag {
    fn __repr__(&self) -> String {
        format!("[{}] {} = {}", self.group, self.name, self.value)
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }

    fn __eq__(&self, other: &Tag) -> bool {
        self.group == other.group && self.name == other.name && self.value == other.value
    }

    fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.group.hash(&mut h);
        self.name.hash(&mut h);
        self.value.hash(&mut h);
        h.finish()
    }
}

/// GPS position in decimal degrees (WGS84). Altitude may be None.
#[pyclass(frozen, get_all, skip_from_py_object)]
#[derive(Clone)]
struct GpsCoordinates {
    latitude: f64,
    longitude: f64,
    /// Meters above sea level, or None if unavailable.
    altitude: Option<f64>,
}

#[pymethods]
impl GpsCoordinates {
    fn __repr__(&self) -> String {
        match self.altitude {
            Some(alt) => format!("{:.6}, {:.6}, {:.1}m", self.latitude, self.longitude, alt),
            None => format!("{:.6}, {:.6}", self.latitude, self.longitude),
        }
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }

    fn __eq__(&self, other: &GpsCoordinates) -> bool {
        self.latitude == other.latitude
            && self.longitude == other.longitude
            && self.altitude == other.altitude
    }
}

/// An image extracted from a PDF, with its pixel geometry and bytes.
#[pyclass(frozen)]
struct ExtractedImage {
    #[pyo3(get)]
    page: u32,
    #[pyo3(get)]
    width: u32,
    #[pyo3(get)]
    height: u32,
    #[pyo3(get)]
    bits_per_component: u8,
    #[pyo3(get)]
    components: u8,
    #[pyo3(get)]
    format: ImageFormat,
    #[pyo3(get)]
    data: Py<PyAny>,
    #[pyo3(get)]
    extension: String,
    #[pyo3(get)]
    is_passthrough: bool,
}

/// A single AcroForm field.
#[pyclass(frozen, get_all, skip_from_py_object)]
#[derive(Clone)]
struct FormField {
    field_type: String,
    name: String,
    value: Option<String>,
    default_value: Option<String>,
    flags: u32,
    is_read_only: bool,
    is_required: bool,
    options: Vec<String>,
    children: Vec<FormField>,
}

#[pymethods]
impl FormField {
    fn __repr__(&self) -> String {
        match &self.value {
            Some(v) => format!("{}: {} = {}", self.field_type, self.name, v),
            None => format!("{}: {}", self.field_type, self.name),
        }
    }
}

/// A PDF interactive form: its fields and document-level flags.
#[pyclass(frozen, get_all, skip_from_py_object)]
#[derive(Clone)]
struct AcroForm {
    fields: Vec<FormField>,
    need_appearances: bool,
    sig_flags: u32,
    total_field_count: usize,
}

/// A PDF annotation - link, highlight, widget, and so on.
#[pyclass(frozen, get_all, skip_from_py_object)]
#[derive(Clone)]
struct Annotation {
    annot_type: String,
    page: usize,
    rect: [f64; 4],
    contents: Option<String>,
    dest: Option<String>,
    name: Option<String>,
    modified: Option<String>,
    flags: u32,
    has_appearance: bool,
}

#[pymethods]
impl Annotation {
    fn __repr__(&self) -> String {
        format!("{} on page {}", self.annot_type, self.page + 1)
    }
}

/// One node of a tagged PDF structure tree.
#[pyclass(frozen, get_all, skip_from_py_object)]
#[derive(Clone)]
struct StructElement {
    struct_type: String,
    title: Option<String>,
    alt_text: Option<String>,
    actual_text: Option<String>,
    lang: Option<String>,
    children: Vec<StructChild>,
}

// Use an enum-like approach for Python
/// A child of a structure element: another element, or a content reference.
#[pyclass(frozen, get_all, skip_from_py_object)]
#[derive(Clone)]
struct StructChild {
    /// "element", "content_ref", or "object_ref"
    child_type: String,
    /// Present when child_type == "element"
    element: Option<StructElement>,
    /// MCID value when child_type == "content_ref"
    mcid: Option<u32>,
    /// Object number when child_type == "object_ref"
    obj_num: Option<u32>,
}

/// The structure tree of a tagged PDF.
#[pyclass(frozen, get_all, skip_from_py_object)]
#[derive(Clone)]
struct StructTree {
    root_elements: Vec<StructElement>,
    role_map: Vec<(String, String)>,
}

// ---------------------------------------------------------------------------
// Owned document wrapper (handles lifetime erasure)
// ---------------------------------------------------------------------------

struct OwnedDocument {
    _data: OwnedData,
    inner: siftx_rs::SiftDocument<'static>,
}

/// Both variants are write-only on purpose: they exist solely to keep the
/// backing memory alive for as long as the transmuted `'static` document
/// borrows from it. Dropping either field would be a use-after-free.
#[allow(dead_code)]
enum OwnedData {
    File(siftx_rs::SiftFile),
    Buffer(Vec<u8>),
}

impl OwnedDocument {
    fn from_file(file: siftx_rs::SiftFile) -> Result<Self, PyErr> {
        let doc = file.parse().map_err(map_err)?;
        let doc: siftx_rs::SiftDocument<'static> = unsafe { std::mem::transmute(doc) };
        Ok(Self {
            _data: OwnedData::File(file),
            inner: doc,
        })
    }

    fn from_buffer(data: Vec<u8>) -> Result<Self, PyErr> {
        let borrowed: &[u8] = &data;
        let borrowed_static: &'static [u8] = unsafe { std::mem::transmute(borrowed) };
        let doc = siftx_rs::read(borrowed_static).map_err(map_err)?;
        Ok(Self {
            _data: OwnedData::Buffer(data),
            inner: doc,
        })
    }
}

// ---------------------------------------------------------------------------
// SiftFile class
// ---------------------------------------------------------------------------

/// An open file. Parse it with `parse()`; usable as a context manager.
#[pyclass]
struct SiftFile {
    file: Option<siftx_rs::SiftFile>,
}

#[pymethods]
impl SiftFile {
    /// Open a file by path (memory-mapped).
    #[staticmethod]
    fn open(path: &str) -> PyResult<SiftFile> {
        let file = siftx_rs::open(path).map_err(map_err)?;
        Ok(SiftFile { file: Some(file) })
    }

    /// Detected file type.
    #[getter]
    fn file_type(&self) -> PyResult<FileType> {
        let file = self.file.as_ref().ok_or_else(|| {
            PyRuntimeError::new_err("SiftFile is closed")
        })?;
        Ok(to_py_file_type(file.file_type()))
    }

    /// Parse into a SiftDocument.
    fn parse(&mut self) -> PyResult<SiftDocument> {
        let file = self.file.take().ok_or_else(|| {
            PyRuntimeError::new_err("SiftFile is closed")
        })?;
        let owned = OwnedDocument::from_file(file)?;
        Ok(SiftDocument {
            inner: Some(owned),
        })
    }

    /// Release the memory mapping.
    fn close(&mut self) {
        self.file = None;
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __exit__(
        &mut self,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_val: Option<&Bound<'_, PyAny>>,
        _exc_tb: Option<&Bound<'_, PyAny>>,
    ) {
        self.close();
    }
}

// ---------------------------------------------------------------------------
// SiftDocument class
// ---------------------------------------------------------------------------

/// A parsed document. Query tags, text, images and PDF structure from it.
#[pyclass]
struct SiftDocument {
    inner: Option<OwnedDocument>,
}

impl SiftDocument {
    fn doc(&self) -> PyResult<&siftx_rs::SiftDocument<'static>> {
        self.inner
            .as_ref()
            .map(|o| &o.inner)
            .ok_or_else(|| PyRuntimeError::new_err("SiftDocument is closed"))
    }

    fn doc_mut(&mut self) -> PyResult<&mut siftx_rs::SiftDocument<'static>> {
        self.inner
            .as_mut()
            .map(|o| &mut o.inner)
            .ok_or_else(|| PyRuntimeError::new_err("SiftDocument is closed"))
    }
}

#[pymethods]
impl SiftDocument {
    /// Detected file type.
    #[getter]
    fn file_type(&self) -> PyResult<FileType> {
        Ok(to_py_file_type(self.doc()?.file_type()))
    }

    /// All metadata tags.
    fn tags(&self) -> PyResult<Vec<Tag>> {
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

    /// GPS coordinates, or None if not present.
    fn gps(&self) -> PyResult<Option<GpsCoordinates>> {
        let gps = self.doc()?.gps();
        Ok(gps.map(|g| GpsCoordinates {
            latitude: g.latitude,
            longitude: g.longitude,
            altitude: g.altitude,
        }))
    }

    /// EXIF thumbnail bytes (JPEG), or None.
    fn thumbnail<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyBytes>>> {
        let thumb = self.doc()?.thumbnail();
        Ok(thumb.map(|bytes| PyBytes::new(py, &bytes)))
    }

    /// Extracted PDF images.
    fn images(&self, py: Python<'_>) -> PyResult<Vec<ExtractedImage>> {
        let images = self.doc()?.images().map_err(map_err)?;
        Ok(images
            .into_iter()
            .map(|img| {
                let ext = img.extension().to_string();
                let is_passthrough = matches!(
                    &img.data,
                    siftx_rs::ImageData::Jpeg(_) | siftx_rs::ImageData::Jpeg2000(_)
                );
                let (format, data) = match img.data {
                    siftx_rs::ImageData::Jpeg(d) => (ImageFormat::Jpeg, d),
                    siftx_rs::ImageData::Jpeg2000(d) => (ImageFormat::Jpeg2000, d),
                    siftx_rs::ImageData::Jbig2 { data: d, .. } => (ImageFormat::Jbig2, d),
                    siftx_rs::ImageData::Ccitt(d) => (ImageFormat::Ccitt, d),
                    siftx_rs::ImageData::Pixels(d) => (ImageFormat::Pixels, d),
                };
                ExtractedImage {
                    page: img.page,
                    width: img.width,
                    height: img.height,
                    bits_per_component: img.bpc,
                    components: img.components,
                    format,
                    data: PyBytes::new(py, &data).into_any().unbind(),
                    extension: ext,
                    is_passthrough,
                }
            })
            .collect())
    }

    /// Text per PDF page.
    fn text_pages(&self) -> PyResult<Vec<String>> {
        self.doc()?.text_pages().map_err(map_err)
    }

    /// EXIF tags only.
    fn exif_tags(&self) -> PyResult<Vec<Tag>> {
        Ok(self.doc()?.exif_tags().into_iter().map(|t| Tag { group: t.group.to_string(), name: t.name, value: t.value }).collect())
    }

    /// XMP tags only.
    fn xmp_tags(&self) -> PyResult<Vec<Tag>> {
        Ok(self.doc()?.xmp_tags().into_iter().map(|t| Tag { group: t.group.to_string(), name: t.name, value: t.value }).collect())
    }

    /// IPTC tags only.
    fn iptc_tags(&self) -> PyResult<Vec<Tag>> {
        Ok(self.doc()?.iptc_tags().into_iter().map(|t| Tag { group: t.group.to_string(), name: t.name, value: t.value }).collect())
    }

    /// Raw text per PDF page (no layout reconstruction).
    fn text_pages_raw(&self) -> PyResult<Vec<String>> {
        self.doc()?.text_pages_raw().map_err(map_err)
    }

    /// Authenticate encrypted PDF. Returns True if password accepted.
    fn authenticate(&mut self, password: &[u8]) -> PyResult<bool> {
        Ok(self.doc_mut()?.authenticate(password))
    }

    /// PDF form fields, or None if no form.
    fn acro_form(&self) -> PyResult<Option<AcroForm>> {
        let form = self.doc()?.acro_form().map_err(map_err)?;
        Ok(form.map(|f| {
            let total = f.total_field_count();
            AcroForm {
                fields: f.fields.iter().map(convert_form_field).collect(),
                need_appearances: f.need_appearances,
                sig_flags: f.sig_flags,
                total_field_count: total,
            }
        }))
    }

    /// All PDF annotations.
    fn annotations(&self) -> PyResult<Vec<Annotation>> {
        let annots = self.doc()?.all_annotations().map_err(map_err)?;
        Ok(annots.into_iter().map(|a| Annotation {
            annot_type: a.annot_type.name().to_string(),
            page: a.page_index,
            rect: a.rect,
            contents: a.contents,
            dest: a.dest,
            name: a.name,
            modified: a.modified,
            flags: a.flags,
            has_appearance: a.has_appearance,
        }).collect())
    }

    /// Tagged PDF structure tree, or None if not tagged.
    fn struct_tree(&self) -> PyResult<Option<StructTree>> {
        let tree = self.doc()?.struct_tree().map_err(map_err)?;
        Ok(tree.map(|t| StructTree {
            root_elements: t.root_elements.iter().map(convert_struct_element).collect(),
            role_map: t.role_map,
        }))
    }

    /// Release native resources.
    fn close(&mut self) {
        self.inner = None;
    }

    fn __enter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __exit__(
        &mut self,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_val: Option<&Bound<'_, PyAny>>,
        _exc_tb: Option<&Bound<'_, PyAny>>,
    ) {
        self.close();
    }
}

// ---------------------------------------------------------------------------
// Convenience functions
// ---------------------------------------------------------------------------

/// Extract tags from a file path in one call.
#[pyfunction]
fn tags(path: &str) -> PyResult<Vec<Tag>> {
    let rust_tags = siftx_rs::tags(path).map_err(map_err)?;
    Ok(rust_tags
        .into_iter()
        .map(|t| Tag {
            group: t.group.to_string(),
            name: t.name,
            value: t.value,
        })
        .collect())
}

/// Parse a document from bytes.
#[pyfunction]
fn read(data: &[u8]) -> PyResult<SiftDocument> {
    let owned = OwnedDocument::from_buffer(data.to_vec())?;
    Ok(SiftDocument {
        inner: Some(owned),
    })
}

/// Native library version.
#[pyfunction]
fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ---------------------------------------------------------------------------
// Helper conversion functions
// ---------------------------------------------------------------------------

fn convert_form_field(f: &siftx_rs::FormField) -> FormField {
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

fn convert_struct_element(e: &siftx_rs::StructElement) -> StructElement {
    StructElement {
        struct_type: e.struct_type.clone(),
        title: e.title.clone(),
        alt_text: e.alt_text.clone(),
        actual_text: e.actual_text.clone(),
        lang: e.lang.clone(),
        children: e.children.iter().map(convert_struct_child).collect(),
    }
}

fn convert_struct_child(c: &siftx_rs::StructChild) -> StructChild {
    match c {
        siftx_rs::StructChild::Element(e) => StructChild {
            child_type: "element".to_string(),
            element: Some(convert_struct_element(e)),
            mcid: None,
            obj_num: None,
        },
        siftx_rs::StructChild::ContentRef(r) => StructChild {
            child_type: "content_ref".to_string(),
            element: None,
            mcid: Some(r.mcid),
            obj_num: None,
        },
        siftx_rs::StructChild::ObjectRef(n) => StructChild {
            child_type: "object_ref".to_string(),
            element: None,
            mcid: None,
            obj_num: Some(*n),
        },
    }
}

// ---------------------------------------------------------------------------
// Module
// ---------------------------------------------------------------------------

/// Cross-platform document and image processing.
///
/// Extracts metadata from images (EXIF, IPTC, XMP, ICC, maker notes) and text,
/// images and metadata from PDFs.
///
///     >>> import siftx
///     >>> for tag in siftx.tags("photo.jpg"):
///     ...     print(tag.group, tag.name, tag.value)
///
///     >>> with siftx.SiftFile.open("document.pdf") as f:
///     ...     doc = f.parse()
///     ...     text = doc.text_pages()
///
/// `tags()` and `read()` are conveniences; `SiftFile.open()` gives a handle you
/// can parse once and query repeatedly. Both the file and the document are
/// context managers, and release native memory on exit.
#[pymodule]
fn siftx(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<FileType>()?;
    m.add_class::<ImageFormat>()?;
    m.add_class::<Tag>()?;
    m.add_class::<GpsCoordinates>()?;
    m.add_class::<ExtractedImage>()?;
    m.add_class::<FormField>()?;
    m.add_class::<AcroForm>()?;
    m.add_class::<Annotation>()?;
    m.add_class::<StructElement>()?;
    m.add_class::<StructChild>()?;
    m.add_class::<StructTree>()?;
    m.add_class::<SiftFile>()?;
    m.add_class::<SiftDocument>()?;
    m.add_function(wrap_pyfunction!(tags, m)?)?;
    m.add_function(wrap_pyfunction!(read, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add("SiftError", m.py().get_type::<SiftError>())?;
    m.add("SiftIOError", m.py().get_type::<SiftIOError>())?;
    m.add("SiftFormatError", m.py().get_type::<SiftFormatError>())?;
    Ok(())
}
