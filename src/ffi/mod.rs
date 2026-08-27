//! C ABI layer for language bindings.
//!
//! All public functions are `extern "C"` with `#[unsafe(no_mangle)]` for use from
//! C, C++, C# (P/Invoke), Java (Panama/JNI), and other FFI consumers.
//!
//! # Memory model
//!
//! - SiftX allocates, caller frees via the matching `siftx_*_free` function.
//! - All returned pointers are owned by the caller until freed.
//! - Strings are NUL-terminated UTF-8 (`*const c_char`).
//! - Errors are reported via `SiftXResult` enum; detail string via `siftx_error_message()`.
//!
//! # Thread safety
//!
//! - `SiftXFile` and `SiftXDocument` are `Send` but not `Sync`.
//! - Each handle must be used from one thread at a time.
//! - Multiple independent handles can be used concurrently.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use std::slice;

use crate::api::{self, Image, ImageData, Tag};

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

/// Result codes returned by all FFI functions.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiftXResult {
    /// Success.
    Ok = 0,
    /// Invalid argument (null pointer, etc.).
    InvalidArg = 1,
    /// I/O error (file not found, permission denied, etc.).
    IoError = 2,
    /// Format error (corrupt or unrecognized file).
    FormatError = 3,
    /// Data truncated unexpectedly.
    Truncated = 4,
    /// Feature not supported.
    Unsupported = 5,
    /// Internal error.
    InternalError = 6,
}

impl From<&crate::core::Error> for SiftXResult {
    fn from(e: &crate::core::Error) -> Self {
        match e {
            crate::core::Error::Io(_) => SiftXResult::IoError,
            crate::core::Error::Format(_) => SiftXResult::FormatError,
            crate::core::Error::Truncated { .. } => SiftXResult::Truncated,
            crate::core::Error::Unsupported(_) => SiftXResult::Unsupported,
            crate::core::Error::Cycle(_) => SiftXResult::FormatError,
        }
    }
}

// Thread-local last error message for retrieval via siftx_error_message().
thread_local! {
    static LAST_ERROR: std::cell::RefCell<Option<CString>> = const { std::cell::RefCell::new(None) };
}

fn set_last_error(msg: &str) {
    let c =
        CString::new(msg).unwrap_or_else(|_| CString::new("error (contains null byte)").unwrap());
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(c));
}

fn result_from_error(e: &crate::core::Error) -> SiftXResult {
    set_last_error(&e.to_string());
    SiftXResult::from(e)
}

/// Get the last error message. Returns NULL if no error.
///
/// The returned pointer is valid until the next FFI call on the same thread.
/// Do NOT free it.
#[unsafe(no_mangle)]
pub extern "C" fn siftx_error_message() -> *const c_char {
    LAST_ERROR.with(|e| match e.borrow().as_ref() {
        Some(c) => c.as_ptr(),
        None => ptr::null(),
    })
}

// ---------------------------------------------------------------------------
// Opaque handles
// ---------------------------------------------------------------------------

/// Opaque handle to a memory-mapped file.
///
/// Created by `siftx_open()`. Must be freed with `siftx_file_free()`.
/// The file must outlive any `SiftXDocument` created from it.
pub struct SiftXFile {
    inner: api::SiftFile,
}

/// Opaque handle to a parsed document.
///
/// Created by `siftx_parse()` or `siftx_read()`. Must be freed with `siftx_document_free()`.
/// For `siftx_parse()`, the parent `SiftXFile` must outlive this document.
pub struct SiftXDocument {
    // We store the document with a 'static lifetime by holding onto the data.
    // For siftx_parse(): data is borrowed from SiftXFile (caller must keep it alive).
    // For siftx_read(): we own a copy of the data.
    _owned_data: Option<Vec<u8>>,
    // SAFETY: The document borrows from either SiftXFile (kept alive by caller)
    // or _owned_data (kept alive by this struct). We use 'static and trust the
    // caller to uphold the lifetime contract for siftx_parse().
    inner: api::SiftDocument<'static>,
}

// ---------------------------------------------------------------------------
// Tag array
// ---------------------------------------------------------------------------

/// Raw value type discriminant for typed tag access.
/// String (0) means no typed value - read the display string.
#[repr(u8)]
#[allow(dead_code)]
pub enum SiftXValueType {
    String = 0,
    U8 = 1,
    U16 = 2,
    U32 = 3,
    U64 = 4,
    I8 = 5,
    I16 = 6,
    I32 = 7,
    I64 = 8,
    F32 = 9,
    F64 = 10,
    Rational = 11,
    SRational = 12,
}

/// A single metadata tag (C-compatible).
///
/// The `value` field always contains a display-ready string.
/// For EXIF tags the typed fields (`value_type`, `int_val`, `rational_num`,
/// `rational_den`, `float_val`) carry the raw parsed value so bindings can
/// provide strongly-typed access without parsing the display string.
#[repr(C)]
pub struct SiftXTag {
    /// Tag group (e.g., "EXIF", "XMP", "PDF"). NUL-terminated UTF-8.
    pub group: *const c_char,
    /// Tag name. NUL-terminated UTF-8.
    pub name: *const c_char,
    /// Display-ready tag value. NUL-terminated UTF-8.
    pub value: *const c_char,
    /// Value type discriminant (0 = string-only, see `SiftXValueType`).
    pub value_type: u8,
    pub _pad: [u8; 3],
    /// Integer value (widened from u8/u16/u32/u64/i8/i16/i32/i64).
    pub int_val: i64,
    /// Rational numerator (for Rational / SRational tags).
    pub rational_num: i32,
    /// Rational denominator (for Rational / SRational tags).
    pub rational_den: i32,
    /// Float value (f32 widened to f64; also precomputed for rationals).
    pub float_val: f64,
}

/// An array of tags returned by `siftx_tags()`.
pub struct SiftXTagArray {
    tags: Vec<SiftXTagOwned>,
}

/// Internal: owns the CStrings for a tag.
struct SiftXTagOwned {
    group: CString,
    name: CString,
    value: CString,
    value_type: u8,
    int_val: i64,
    rational_num: i32,
    rational_den: i32,
    float_val: f64,
}

impl SiftXTagOwned {
    fn from_tag(tag: &Tag) -> Self {
        use crate::core::TagValue;

        let (vt, iv, rn, rd, fv) = match &tag.typed_value {
            Some(TagValue::U8(v)) => (1, *v as i64, 0, 0, *v as f64),
            Some(TagValue::U16(v)) => (2, *v as i64, 0, 0, *v as f64),
            Some(TagValue::U32(v)) => (3, *v as i64, 0, 0, *v as f64),
            Some(TagValue::U64(v)) => (4, *v as i64, 0, 0, *v as f64),
            Some(TagValue::I8(v)) => (5, *v as i64, 0, 0, *v as f64),
            Some(TagValue::I16(v)) => (6, *v as i64, 0, 0, *v as f64),
            Some(TagValue::I32(v)) => (7, *v as i64, 0, 0, *v as f64),
            Some(TagValue::I64(v)) => (8, *v, 0, 0, *v as f64),
            Some(TagValue::F32(v)) => (9, 0, 0, 0, *v as f64),
            Some(TagValue::F64(v)) => (10, 0, 0, 0, *v),
            Some(TagValue::Rational(n, d)) => (
                11,
                0,
                *n as i32,
                *d as i32,
                if *d != 0 { *n as f64 / *d as f64 } else { 0.0 },
            ),
            Some(TagValue::SRational(n, d)) => (
                12,
                0,
                *n,
                *d,
                if *d != 0 { *n as f64 / *d as f64 } else { 0.0 },
            ),
            // Ascii is already in the display string; arrays default to string.
            _ => (0, 0, 0, 0, 0.0),
        };

        Self {
            group: CString::new(tag.group).unwrap_or_default(),
            name: CString::new(tag.name.as_str()).unwrap_or_default(),
            value: CString::new(tag.value.as_str()).unwrap_or_default(),
            value_type: vt,
            int_val: iv,
            rational_num: rn,
            rational_den: rd,
            float_val: fv,
        }
    }

    fn as_c_tag(&self) -> SiftXTag {
        SiftXTag {
            group: self.group.as_ptr(),
            name: self.name.as_ptr(),
            value: self.value.as_ptr(),
            value_type: self.value_type,
            _pad: [0; 3],
            int_val: self.int_val,
            rational_num: self.rational_num,
            rational_den: self.rational_den,
            float_val: self.float_val,
        }
    }
}

// ---------------------------------------------------------------------------
// GPS
// ---------------------------------------------------------------------------

/// GPS coordinates (C-compatible).
#[repr(C)]
pub struct SiftXGps {
    /// Decimal degrees, negative = south.
    pub latitude: f64,
    /// Decimal degrees, negative = west.
    pub longitude: f64,
    /// Altitude in meters (NaN if not available).
    pub altitude: f64,
    /// 1 if altitude is valid, 0 if not.
    pub has_altitude: i32,
}

// ---------------------------------------------------------------------------
// Image array (PDF)
// ---------------------------------------------------------------------------

/// A single extracted image (C-compatible).
#[repr(C)]
pub struct SiftXImage {
    /// 0-based page index.
    pub page: u32,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Bits per component.
    pub bpc: u8,
    /// Number of color components.
    pub components: u8,
    /// Image format: 0=JPEG, 1=JPEG2000, 2=JBIG2, 3=CCITT, 4=Pixels.
    pub format: u8,
    /// Pointer to image data bytes.
    pub data: *const u8,
    /// Length of image data in bytes.
    pub data_len: usize,
}

/// An array of images returned by `siftx_images()`.
pub struct SiftXImageArray {
    images: Vec<Image>,
}

// ---------------------------------------------------------------------------
// Text pages (PDF)
// ---------------------------------------------------------------------------

/// An array of text pages returned by `siftx_text_pages()`.
pub struct SiftXTextPages {
    pages: Vec<CString>,
}

// ---------------------------------------------------------------------------
// File type
// ---------------------------------------------------------------------------

/// Detected file type constants.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiftXFileType {
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

impl From<Option<crate::core::FileType>> for SiftXFileType {
    fn from(ft: Option<crate::core::FileType>) -> Self {
        match ft {
            None => SiftXFileType::Unknown,
            Some(crate::core::FileType::Jpeg) => SiftXFileType::Jpeg,
            Some(crate::core::FileType::Png) => SiftXFileType::Png,
            Some(crate::core::FileType::Gif) => SiftXFileType::Gif,
            Some(crate::core::FileType::Bmp) => SiftXFileType::Bmp,
            Some(crate::core::FileType::Tiff) => SiftXFileType::Tiff,
            Some(crate::core::FileType::WebP) => SiftXFileType::WebP,
            Some(crate::core::FileType::Heif) => SiftXFileType::Heif,
            Some(crate::core::FileType::Pdf) => SiftXFileType::Pdf,
            Some(crate::core::FileType::Icc) => SiftXFileType::Icc,
            Some(crate::core::FileType::QuickTime) => SiftXFileType::QuickTime,
            // RAW camera formats - map to their underlying container type
            Some(crate::core::FileType::Cr2)
            | Some(crate::core::FileType::Nef)
            | Some(crate::core::FileType::Arw)
            | Some(crate::core::FileType::Dng)
            | Some(crate::core::FileType::Orf)
            | Some(crate::core::FileType::Rw2)
            | Some(crate::core::FileType::Pef)
            | Some(crate::core::FileType::Srw)
            | Some(crate::core::FileType::Raf) => SiftXFileType::Tiff,
            Some(crate::core::FileType::Cr3) => SiftXFileType::Heif,
        }
    }
}

// ===========================================================================
// Public FFI functions
// ===========================================================================

// ---------------------------------------------------------------------------
// Lifecycle: open / read / parse / free
// ---------------------------------------------------------------------------

/// Open a file by path and return a handle.
///
/// The path must be a NUL-terminated UTF-8 string.
/// On success, writes the handle to `*out` and returns `SIFTX_OK`.
/// On failure, `*out` is set to NULL.
///
/// The returned handle must be freed with `siftx_file_free()`.
///
/// # Safety
///
/// - `path` must be NULL, or point to a writable, aligned `c_char`.
/// - It must be NUL-terminated.
/// - `out` must be NULL, or point to a writable, aligned `*mut SiftXFile`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_open(path: *const c_char, out: *mut *mut SiftXFile) -> SiftXResult {
    if path.is_null() || out.is_null() {
        set_last_error("null pointer argument");
        if !out.is_null() {
            unsafe {
                *out = ptr::null_mut();
            }
        }
        return SiftXResult::InvalidArg;
    }

    unsafe {
        *out = ptr::null_mut();
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_last_error(&format!("invalid UTF-8 path: {e}"));
            return SiftXResult::InvalidArg;
        }
    };

    match api::open(path_str) {
        Ok(file) => {
            let handle = Box::new(SiftXFile { inner: file });
            unsafe {
                *out = Box::into_raw(handle);
            }
            SiftXResult::Ok
        }
        Err(e) => result_from_error(&e),
    }
}

/// Get the detected file type of an opened file.
///
/// # Safety
///
/// - `file` must be NULL, or a `SiftXFile*` from `siftx_open()` that has not
///   been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_file_type(file: *const SiftXFile) -> SiftXFileType {
    if file.is_null() {
        return SiftXFileType::Unknown;
    }
    SiftXFileType::from(unsafe { &*file }.inner.file_type())
}

/// Parse an opened file into a document handle.
///
/// The `SiftXFile` must remain alive for the lifetime of the returned document.
/// On success, writes the document handle to `*out` and returns `SIFTX_OK`.
///
/// The returned handle must be freed with `siftx_document_free()`.
///
/// # Safety
///
/// - `file` must be NULL, or a `SiftXFile*` from `siftx_open()` that has not
///   been freed.
/// - `out` must be NULL, or point to a writable, aligned `*mut SiftXDocument`.
/// - **The file must outlive the returned document.** The document borrows the
///   file's memory mapping; freeing the file first leaves the document dangling
///   and any later call on it is a use-after-free. Use `siftx_read()` if you
///   want a self-contained document.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_parse(
    file: *const SiftXFile,
    out: *mut *mut SiftXDocument,
) -> SiftXResult {
    if file.is_null() || out.is_null() {
        set_last_error("null pointer argument");
        if !out.is_null() {
            unsafe {
                *out = ptr::null_mut();
            }
        }
        return SiftXResult::InvalidArg;
    }

    unsafe {
        *out = ptr::null_mut();
    }

    let file_ref = unsafe { &*file };
    // SAFETY: We transmute the lifetime to 'static. The caller contract is that
    // the SiftXFile outlives the SiftXDocument. This is documented in siftx.h.
    match file_ref.inner.parse() {
        Ok(doc) => {
            let doc_static: api::SiftDocument<'static> = unsafe { std::mem::transmute(doc) };
            let handle = Box::new(SiftXDocument {
                _owned_data: None,
                inner: doc_static,
            });
            unsafe {
                *out = Box::into_raw(handle);
            }
            SiftXResult::Ok
        }
        Err(e) => result_from_error(&e),
    }
}

/// Parse a byte buffer into a document handle.
///
/// The data is copied internally - the caller can free it after this call returns.
/// On success, writes the document handle to `*out` and returns `SIFTX_OK`.
///
/// The returned handle must be freed with `siftx_document_free()`.
///
/// # Safety
///
/// - `data` must be NULL, or point to `data_len` initialised, readable bytes.
/// - The bytes are copied, so the caller may free them once this returns.
/// - `out` must be NULL, or point to a writable, aligned `*mut SiftXDocument`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_read(
    data: *const u8,
    data_len: usize,
    out: *mut *mut SiftXDocument,
) -> SiftXResult {
    if data.is_null() || out.is_null() {
        set_last_error("null pointer argument");
        if !out.is_null() {
            unsafe {
                *out = ptr::null_mut();
            }
        }
        return SiftXResult::InvalidArg;
    }

    unsafe {
        *out = ptr::null_mut();
    }

    // Copy data so the document owns it.
    let owned = unsafe { slice::from_raw_parts(data, data_len) }.to_vec();

    // Parse from the owned data. We need the borrow to point into our owned Vec,
    // then transmute the lifetime to 'static (safe because _owned_data keeps it alive).
    let borrowed: &[u8] = &owned;
    let borrowed_static: &'static [u8] = unsafe { std::mem::transmute(borrowed) };

    match api::read(borrowed_static) {
        Ok(doc) => {
            let handle = Box::new(SiftXDocument {
                _owned_data: Some(owned),
                inner: doc,
            });
            unsafe {
                *out = Box::into_raw(handle);
            }
            SiftXResult::Ok
        }
        Err(e) => result_from_error(&e),
    }
}

/// Free a file handle. NULL is safely ignored.
///
/// # Safety
///
/// - `file` must be NULL, or a `SiftXFile*` from `siftx_open()`.
/// - It must not have been freed already, and must not be used afterwards.
/// - Any document created from it via `siftx_parse()` must be freed first.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_file_free(file: *mut SiftXFile) {
    if !file.is_null() {
        drop(unsafe { Box::from_raw(file) });
    }
}

/// Free a document handle. NULL is safely ignored.
///
/// # Safety
///
/// - `doc` must be NULL, or a `SiftXDocument*` from `siftx_parse()` or
///   `siftx_read()`.
/// - It must not have been freed already, and must not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_document_free(doc: *mut SiftXDocument) {
    if !doc.is_null() {
        drop(unsafe { Box::from_raw(doc) });
    }
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

/// Extract all metadata tags from a document.
///
/// On success, writes the tag array to `*out` and returns `SIFTX_OK`.
/// Use `siftx_tags_count()`, `siftx_tags_get()` to iterate, and `siftx_tags_free()` to free.
///
/// # Safety
///
/// - `doc` must be NULL, or a `SiftXDocument*` from `siftx_parse()` or
///   `siftx_read()` that has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `*mut SiftXTagArray`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_tags(
    doc: *const SiftXDocument,
    out: *mut *mut SiftXTagArray,
) -> SiftXResult {
    if doc.is_null() || out.is_null() {
        set_last_error("null pointer argument");
        if !out.is_null() {
            unsafe {
                *out = ptr::null_mut();
            }
        }
        return SiftXResult::InvalidArg;
    }

    let doc_ref = unsafe { &*doc };
    let tags = doc_ref.inner.tags();
    let owned: Vec<SiftXTagOwned> = tags.iter().map(SiftXTagOwned::from_tag).collect();

    let handle = Box::new(SiftXTagArray { tags: owned });
    unsafe {
        *out = Box::into_raw(handle);
    }
    SiftXResult::Ok
}

/// Convenience: open a file and extract all tags in one call.
///
/// On success, writes the tag array to `*out` and returns `SIFTX_OK`.
///
/// # Safety
///
/// - `path` must be NULL, or point to a writable, aligned `c_char`.
/// - It must be NUL-terminated.
/// - `out` must be NULL, or point to a writable, aligned `*mut SiftXTagArray`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_tags_from_path(
    path: *const c_char,
    out: *mut *mut SiftXTagArray,
) -> SiftXResult {
    if path.is_null() || out.is_null() {
        set_last_error("null pointer argument");
        if !out.is_null() {
            unsafe {
                *out = ptr::null_mut();
            }
        }
        return SiftXResult::InvalidArg;
    }

    unsafe {
        *out = ptr::null_mut();
    }

    let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_last_error(&format!("invalid UTF-8 path: {e}"));
            return SiftXResult::InvalidArg;
        }
    };

    match api::tags(path_str) {
        Ok(tags) => {
            let owned: Vec<SiftXTagOwned> = tags.iter().map(SiftXTagOwned::from_tag).collect();
            let handle = Box::new(SiftXTagArray { tags: owned });
            unsafe {
                *out = Box::into_raw(handle);
            }
            SiftXResult::Ok
        }
        Err(e) => result_from_error(&e),
    }
}

/// Number of tags in the array.
///
/// # Safety
///
/// - `tags` must be NULL, or a `SiftXTagArray*` from `siftx_tags()`,
///   `siftx_tags_from_path()`, `siftx_exif_tags()`, `siftx_xmp_tags()` or
///   `siftx_iptc_tags()` that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_tags_count(tags: *const SiftXTagArray) -> usize {
    if tags.is_null() {
        return 0;
    }
    unsafe { &*tags }.tags.len()
}

/// Get tag at index. Returns 0 on success, non-zero if index is out of bounds.
///
/// The returned `SiftXTag` pointers are valid until `siftx_tags_free()` is called.
///
/// # Safety
///
/// - `tags` must be NULL, or a `SiftXTagArray*` from `siftx_tags()`,
///   `siftx_tags_from_path()`, `siftx_exif_tags()`, `siftx_xmp_tags()` or
///   `siftx_iptc_tags()` that has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `SiftXTag`.
/// - The pointers written into `*out` borrow the array. They are invalidated by
///   `siftx_tags_free()` - copy anything you need to keep.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_tags_get(
    tags: *const SiftXTagArray,
    index: usize,
    out: *mut SiftXTag,
) -> SiftXResult {
    if tags.is_null() || out.is_null() {
        return SiftXResult::InvalidArg;
    }
    let arr = unsafe { &*tags };
    if index >= arr.tags.len() {
        set_last_error("tag index out of bounds");
        return SiftXResult::InvalidArg;
    }
    unsafe {
        *out = arr.tags[index].as_c_tag();
    }
    SiftXResult::Ok
}

/// Free a tag array. NULL is safely ignored.
///
/// # Safety
///
/// - `tags` must be NULL, or a `SiftXTagArray*` from `siftx_tags()`,
///   `siftx_tags_from_path()`, `siftx_exif_tags()`, `siftx_xmp_tags()` or
///   `siftx_iptc_tags()`.
/// - It must not have been freed already, and neither it nor any pointer
///   obtained from it may be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_tags_free(tags: *mut SiftXTagArray) {
    if !tags.is_null() {
        drop(unsafe { Box::from_raw(tags) });
    }
}

// ---------------------------------------------------------------------------
// GPS
// ---------------------------------------------------------------------------

/// Extract GPS coordinates from a document.
///
/// Returns `SIFTX_OK` if GPS data was found and written to `*out`.
/// Returns `SIFTX_UNSUPPORTED` if no GPS data is present (not an error).
///
/// # Safety
///
/// - `doc` must be NULL, or a `SiftXDocument*` from `siftx_parse()` or
///   `siftx_read()` that has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `SiftXGps`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_gps(doc: *const SiftXDocument, out: *mut SiftXGps) -> SiftXResult {
    if doc.is_null() || out.is_null() {
        set_last_error("null pointer argument");
        return SiftXResult::InvalidArg;
    }

    let doc_ref = unsafe { &*doc };
    match doc_ref.inner.gps() {
        Some(gps) => {
            unsafe {
                *out = SiftXGps {
                    latitude: gps.latitude,
                    longitude: gps.longitude,
                    altitude: gps.altitude.unwrap_or(f64::NAN),
                    has_altitude: if gps.altitude.is_some() { 1 } else { 0 },
                };
            }
            SiftXResult::Ok
        }
        None => {
            set_last_error("no GPS data found");
            SiftXResult::Unsupported
        }
    }
}

// ---------------------------------------------------------------------------
// Thumbnail
// ---------------------------------------------------------------------------

/// Extract the EXIF thumbnail (IFD1 JPEG) from a document.
///
/// On success, writes the JPEG data pointer to `*out_data` and length to `*out_len`.
/// The data must be freed with `siftx_thumbnail_free()`.
///
/// Returns `SIFTX_UNSUPPORTED` if no thumbnail is present.
///
/// # Safety
///
/// - `doc` must be NULL, or a `SiftXDocument*` from `siftx_parse()` or
///   `siftx_read()` that has not been freed.
/// - `out_data` must be NULL, or point to a writable, aligned `*const u8`.
/// - `out_len` must be NULL, or point to a writable, aligned `usize`.
/// - On success `*out_data` is an owned allocation that must be released with
///   `siftx_thumbnail_free()`, passing back the same pointer and length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_thumbnail(
    doc: *const SiftXDocument,
    out_data: *mut *const u8,
    out_len: *mut usize,
) -> SiftXResult {
    if doc.is_null() || out_data.is_null() || out_len.is_null() {
        set_last_error("null pointer argument");
        return SiftXResult::InvalidArg;
    }

    unsafe {
        *out_data = ptr::null();
        *out_len = 0;
    }

    let doc_ref = unsafe { &*doc };

    #[cfg(feature = "tiff")]
    {
        if let Some(thumb) = doc_ref.inner.thumbnail() {
            let boxed = thumb.into_boxed_slice();
            let len = boxed.len();
            let ptr = Box::into_raw(boxed) as *const u8;
            unsafe {
                *out_data = ptr;
                *out_len = len;
            }
            return SiftXResult::Ok;
        }
    }

    set_last_error("no thumbnail found");
    SiftXResult::Unsupported
}

/// Free thumbnail data. NULL is safely ignored.
///
/// # Safety
///
/// - `data` must be NULL, or exactly the pointer written to `*out_data` by
///   `siftx_thumbnail()`, with `len` exactly the length written alongside it.
/// - A different length reconstructs an allocation of the wrong size, which
///   corrupts the allocator. The pointer must not be freed twice or used after.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_thumbnail_free(data: *mut u8, len: usize) {
    if !data.is_null() && len > 0 {
        drop(unsafe { Box::from_raw(slice::from_raw_parts_mut(data, len)) });
    }
}

// ---------------------------------------------------------------------------
// PDF: Images
// ---------------------------------------------------------------------------

/// Extract all images from a PDF document.
///
/// On success, writes the image array to `*out` and returns `SIFTX_OK`.
/// For non-PDF documents, returns `SIFTX_OK` with an empty array.
///
/// Use `siftx_images_count()`, `siftx_images_get()`, and `siftx_images_free()`.
///
/// # Safety
///
/// - `doc` must be NULL, or a `SiftXDocument*` from `siftx_parse()` or
///   `siftx_read()` that has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `*mut
///   SiftXImageArray`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_images(
    doc: *const SiftXDocument,
    out: *mut *mut SiftXImageArray,
) -> SiftXResult {
    if doc.is_null() || out.is_null() {
        set_last_error("null pointer argument");
        if !out.is_null() {
            unsafe {
                *out = ptr::null_mut();
            }
        }
        return SiftXResult::InvalidArg;
    }

    unsafe {
        *out = ptr::null_mut();
    }

    let doc_ref = unsafe { &*doc };

    #[cfg(feature = "pdf")]
    {
        match doc_ref.inner.images() {
            Ok(images) => {
                let handle = Box::new(SiftXImageArray { images });
                unsafe {
                    *out = Box::into_raw(handle);
                }
                return SiftXResult::Ok;
            }
            Err(e) => return result_from_error(&e),
        }
    }

    #[cfg(not(feature = "pdf"))]
    {
        let handle = Box::new(SiftXImageArray { images: Vec::new() });
        unsafe {
            *out = Box::into_raw(handle);
        }
        SiftXResult::Ok
    }
}

/// Number of images in the array.
///
/// # Safety
///
/// - `images` must be NULL, or a `SiftXImageArray*` from `siftx_images()` that
///   has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_images_count(images: *const SiftXImageArray) -> usize {
    if images.is_null() {
        return 0;
    }
    unsafe { &*images }.images.len()
}

/// Get image metadata at index.
///
/// The `data` pointer in the returned `SiftXImage` is valid until `siftx_images_free()`.
///
/// # Safety
///
/// - `images` must be NULL, or a `SiftXImageArray*` from `siftx_images()` that
///   has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `SiftXImage`.
/// - The pointers written into `*out` borrow the array. They are invalidated by
///   `siftx_images_free()` - copy anything you need to keep.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_images_get(
    images: *const SiftXImageArray,
    index: usize,
    out: *mut SiftXImage,
) -> SiftXResult {
    if images.is_null() || out.is_null() {
        return SiftXResult::InvalidArg;
    }
    let arr = unsafe { &*images };
    if index >= arr.images.len() {
        set_last_error("image index out of bounds");
        return SiftXResult::InvalidArg;
    }
    let img = &arr.images[index];
    let (format, data_ptr, data_len) = match &img.data {
        ImageData::Jpeg(d) => (0u8, d.as_ptr(), d.len()),
        ImageData::Jpeg2000(d) => (1u8, d.as_ptr(), d.len()),
        ImageData::Jbig2 { data, .. } => (2u8, data.as_ptr(), data.len()),
        ImageData::Ccitt(d) => (3u8, d.as_ptr(), d.len()),
        ImageData::Pixels(d) => (4u8, d.as_ptr(), d.len()),
    };
    unsafe {
        *out = SiftXImage {
            page: img.page,
            width: img.width,
            height: img.height,
            bpc: img.bpc,
            components: img.components,
            format,
            data: data_ptr,
            data_len,
        };
    }
    SiftXResult::Ok
}

/// Free an image array. NULL is safely ignored.
///
/// # Safety
///
/// - `images` must be NULL, or a `SiftXImageArray*` from `siftx_images()`.
/// - It must not have been freed already, and neither it nor any pointer
///   obtained from it may be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_images_free(images: *mut SiftXImageArray) {
    if !images.is_null() {
        drop(unsafe { Box::from_raw(images) });
    }
}

// ---------------------------------------------------------------------------
// PDF: Text pages
// ---------------------------------------------------------------------------

/// Extract text from all pages of a PDF document.
///
/// On success, writes the text pages array to `*out` and returns `SIFTX_OK`.
/// For non-PDF documents, returns `SIFTX_OK` with an empty array.
///
/// Use `siftx_text_pages_count()`, `siftx_text_pages_get()`, and `siftx_text_pages_free()`.
///
/// # Safety
///
/// - `doc` must be NULL, or a `SiftXDocument*` from `siftx_parse()` or
///   `siftx_read()` that has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `*mut SiftXTextPages`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_text_pages(
    doc: *const SiftXDocument,
    out: *mut *mut SiftXTextPages,
) -> SiftXResult {
    if doc.is_null() || out.is_null() {
        set_last_error("null pointer argument");
        if !out.is_null() {
            unsafe {
                *out = ptr::null_mut();
            }
        }
        return SiftXResult::InvalidArg;
    }

    unsafe {
        *out = ptr::null_mut();
    }

    let doc_ref = unsafe { &*doc };

    #[cfg(feature = "pdf")]
    {
        match doc_ref.inner.text_pages() {
            Ok(pages) => {
                let c_pages: Vec<CString> = pages
                    .into_iter()
                    .map(|s| CString::new(s).unwrap_or_default())
                    .collect();
                let handle = Box::new(SiftXTextPages { pages: c_pages });
                unsafe {
                    *out = Box::into_raw(handle);
                }
                return SiftXResult::Ok;
            }
            Err(e) => return result_from_error(&e),
        }
    }

    #[cfg(not(feature = "pdf"))]
    {
        let handle = Box::new(SiftXTextPages { pages: Vec::new() });
        unsafe {
            *out = Box::into_raw(handle);
        }
        SiftXResult::Ok
    }
}

/// Number of text pages.
///
/// # Safety
///
/// - `pages` must be NULL, or a `SiftXTextPages*` from `siftx_text_pages()` or
///   `siftx_text_pages_raw()` that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_text_pages_count(pages: *const SiftXTextPages) -> usize {
    if pages.is_null() {
        return 0;
    }
    unsafe { &*pages }.pages.len()
}

/// Get text of page at index. Returns a NUL-terminated UTF-8 string.
///
/// The returned pointer is valid until `siftx_text_pages_free()`.
///
/// # Safety
///
/// - `pages` must be NULL, or a `SiftXTextPages*` from `siftx_text_pages()` or
///   `siftx_text_pages_raw()` that has not been freed.
/// - The returned string borrows the array and is invalidated by
///   `siftx_text_pages_free()` - copy it if you need it to outlive the array.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_text_pages_get(
    pages: *const SiftXTextPages,
    index: usize,
) -> *const c_char {
    if pages.is_null() {
        return ptr::null();
    }
    let arr = unsafe { &*pages };
    if index >= arr.pages.len() {
        return ptr::null();
    }
    arr.pages[index].as_ptr()
}

/// Free a text pages array. NULL is safely ignored.
///
/// # Safety
///
/// - `pages` must be NULL, or a `SiftXTextPages*` from `siftx_text_pages()` or
///   `siftx_text_pages_raw()`.
/// - It must not have been freed already, and neither it nor any pointer
///   obtained from it may be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_text_pages_free(pages: *mut SiftXTextPages) {
    if !pages.is_null() {
        drop(unsafe { Box::from_raw(pages) });
    }
}

// ---------------------------------------------------------------------------
// Filtered tags
// ---------------------------------------------------------------------------

/// Extract only EXIF tags from a document.
///
/// # Safety
///
/// - `doc` must be NULL, or a `SiftXDocument*` from `siftx_parse()` or
///   `siftx_read()` that has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `*mut SiftXTagArray`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_exif_tags(
    doc: *const SiftXDocument,
    out: *mut *mut SiftXTagArray,
) -> SiftXResult {
    extract_filtered_tags(doc, out, |d| d.exif_tags())
}

/// Extract only XMP tags from a document.
///
/// # Safety
///
/// - `doc` must be NULL, or a `SiftXDocument*` from `siftx_parse()` or
///   `siftx_read()` that has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `*mut SiftXTagArray`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_xmp_tags(
    doc: *const SiftXDocument,
    out: *mut *mut SiftXTagArray,
) -> SiftXResult {
    extract_filtered_tags(doc, out, |d| d.xmp_tags())
}

/// Extract only IPTC tags from a document.
///
/// # Safety
///
/// - `doc` must be NULL, or a `SiftXDocument*` from `siftx_parse()` or
///   `siftx_read()` that has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `*mut SiftXTagArray`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_iptc_tags(
    doc: *const SiftXDocument,
    out: *mut *mut SiftXTagArray,
) -> SiftXResult {
    extract_filtered_tags(doc, out, |d| d.iptc_tags())
}

fn extract_filtered_tags(
    doc: *const SiftXDocument,
    out: *mut *mut SiftXTagArray,
    extractor: impl FnOnce(&api::SiftDocument<'_>) -> Vec<Tag>,
) -> SiftXResult {
    if doc.is_null() || out.is_null() {
        set_last_error("null pointer argument");
        if !out.is_null() {
            unsafe {
                *out = ptr::null_mut();
            }
        }
        return SiftXResult::InvalidArg;
    }

    let doc_ref = unsafe { &*doc };
    let tags = extractor(&doc_ref.inner);
    let owned: Vec<SiftXTagOwned> = tags.iter().map(SiftXTagOwned::from_tag).collect();
    let handle = Box::new(SiftXTagArray { tags: owned });
    unsafe {
        *out = Box::into_raw(handle);
    }
    SiftXResult::Ok
}

// ---------------------------------------------------------------------------
// PDF: Raw text pages
// ---------------------------------------------------------------------------

/// Extract raw text (no layout) from all pages of a PDF document.
///
/// Faster than `siftx_text_pages()` but may lose whitespace structure.
///
/// # Safety
///
/// - `doc` must be NULL, or a `SiftXDocument*` from `siftx_parse()` or
///   `siftx_read()` that has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `*mut SiftXTextPages`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_text_pages_raw(
    doc: *const SiftXDocument,
    out: *mut *mut SiftXTextPages,
) -> SiftXResult {
    if doc.is_null() || out.is_null() {
        set_last_error("null pointer argument");
        if !out.is_null() {
            unsafe {
                *out = ptr::null_mut();
            }
        }
        return SiftXResult::InvalidArg;
    }

    unsafe {
        *out = ptr::null_mut();
    }
    let doc_ref = unsafe { &*doc };

    #[cfg(feature = "pdf")]
    {
        match doc_ref.inner.text_pages_raw() {
            Ok(pages) => {
                let c_pages: Vec<CString> = pages
                    .into_iter()
                    .map(|s| CString::new(s).unwrap_or_default())
                    .collect();
                let handle = Box::new(SiftXTextPages { pages: c_pages });
                unsafe {
                    *out = Box::into_raw(handle);
                }
                return SiftXResult::Ok;
            }
            Err(e) => return result_from_error(&e),
        }
    }

    #[cfg(not(feature = "pdf"))]
    {
        let handle = Box::new(SiftXTextPages { pages: Vec::new() });
        unsafe {
            *out = Box::into_raw(handle);
        }
        SiftXResult::Ok
    }
}

// ---------------------------------------------------------------------------
// PDF: Authentication
// ---------------------------------------------------------------------------

/// Authenticate an encrypted PDF with a password.
///
/// Returns 1 if the password was accepted, 0 if wrong or not encrypted.
/// After success, text/image extraction will work on the decrypted content.
///
/// For non-PDF documents, always returns 0.
///
/// # Safety
///
/// - `doc` must be NULL, or a `SiftXDocument*` from `siftx_parse()` or
///   `siftx_read()` that has not been freed. It is mutated in place, so no
///   other thread may be using it concurrently.
/// - `password` must be NULL, or point to `password_len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_authenticate(
    doc: *mut SiftXDocument,
    password: *const u8,
    password_len: usize,
) -> i32 {
    if doc.is_null() || (password.is_null() && password_len > 0) {
        return 0;
    }
    let doc_ref = unsafe { &mut *doc };
    let pwd = if password_len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(password, password_len) }
    };
    if doc_ref.inner.authenticate(pwd) {
        1
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// PDF: Form fields
// ---------------------------------------------------------------------------

/// A single form field (C-compatible, flattened from tree).
#[repr(C)]
pub struct SiftXFormField {
    /// Field type: "Text", "Button", "Choice", "Signature", "Unknown".
    pub field_type: *const c_char,
    /// Fully qualified field name.
    pub name: *const c_char,
    /// Current value, or NULL.
    pub value: *const c_char,
    /// Default value, or NULL.
    pub default_value: *const c_char,
    /// Field flags (/Ff).
    pub flags: u32,
    /// 1 if read-only, 0 otherwise.
    pub is_read_only: i32,
    /// 1 if required, 0 otherwise.
    pub is_required: i32,
}

struct SiftXFormFieldOwned {
    field_type: CString,
    name: CString,
    value: Option<CString>,
    default_value: Option<CString>,
    flags: u32,
    is_read_only: bool,
    is_required: bool,
}

impl SiftXFormFieldOwned {
    fn as_c_field(&self) -> SiftXFormField {
        SiftXFormField {
            field_type: self.field_type.as_ptr(),
            name: self.name.as_ptr(),
            value: self
                .value
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(ptr::null()),
            default_value: self
                .default_value
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(ptr::null()),
            flags: self.flags,
            is_read_only: if self.is_read_only { 1 } else { 0 },
            is_required: if self.is_required { 1 } else { 0 },
        }
    }
}

pub struct SiftXFormFieldArray {
    fields: Vec<SiftXFormFieldOwned>,
}

#[cfg(feature = "pdf")]
fn flatten_form_fields(
    fields: &[crate::pdf::annot::FormField],
    out: &mut Vec<SiftXFormFieldOwned>,
) {
    for f in fields {
        out.push(SiftXFormFieldOwned {
            field_type: CString::new(f.field_type.name()).unwrap_or_default(),
            name: CString::new(f.full_name.as_str()).unwrap_or_default(),
            value: f.value.as_ref().and_then(|v| CString::new(v.as_str()).ok()),
            default_value: f
                .default_value
                .as_ref()
                .and_then(|v| CString::new(v.as_str()).ok()),
            flags: f.flags,
            is_read_only: f.is_read_only(),
            is_required: f.is_required(),
        });
        if !f.children.is_empty() {
            flatten_form_fields(&f.children, out);
        }
    }
}

/// Extract form fields from a PDF document.
///
/// Returns `SIFTX_OK` with an empty array for non-PDF documents or PDFs without forms.
///
/// # Safety
///
/// - `doc` must be NULL, or a `SiftXDocument*` from `siftx_parse()` or
///   `siftx_read()` that has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `*mut
///   SiftXFormFieldArray`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_form_fields(
    doc: *const SiftXDocument,
    out: *mut *mut SiftXFormFieldArray,
) -> SiftXResult {
    if doc.is_null() || out.is_null() {
        set_last_error("null pointer argument");
        if !out.is_null() {
            unsafe {
                *out = ptr::null_mut();
            }
        }
        return SiftXResult::InvalidArg;
    }

    unsafe {
        *out = ptr::null_mut();
    }
    let doc_ref = unsafe { &*doc };

    #[cfg(feature = "pdf")]
    {
        match doc_ref.inner.acro_form() {
            Ok(Some(form)) => {
                let mut fields = Vec::new();
                flatten_form_fields(&form.fields, &mut fields);
                let handle = Box::new(SiftXFormFieldArray { fields });
                unsafe {
                    *out = Box::into_raw(handle);
                }
                return SiftXResult::Ok;
            }
            Ok(None) => {
                let handle = Box::new(SiftXFormFieldArray { fields: Vec::new() });
                unsafe {
                    *out = Box::into_raw(handle);
                }
                return SiftXResult::Ok;
            }
            Err(e) => return result_from_error(&e),
        }
    }

    #[cfg(not(feature = "pdf"))]
    {
        let handle = Box::new(SiftXFormFieldArray { fields: Vec::new() });
        unsafe {
            *out = Box::into_raw(handle);
        }
        SiftXResult::Ok
    }
}

/// Number of form fields.
///
/// # Safety
///
/// - `fields` must be NULL, or a `SiftXFormFieldArray*` from
///   `siftx_form_fields()` that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_form_fields_count(fields: *const SiftXFormFieldArray) -> usize {
    if fields.is_null() {
        return 0;
    }
    unsafe { &*fields }.fields.len()
}

/// Get form field at index.
///
/// # Safety
///
/// - `fields` must be NULL, or a `SiftXFormFieldArray*` from
///   `siftx_form_fields()` that has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `SiftXFormField`.
/// - The pointers written into `*out` borrow the array. They are invalidated by
///   `siftx_form_fields_free()` - copy anything you need to keep.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_form_fields_get(
    fields: *const SiftXFormFieldArray,
    index: usize,
    out: *mut SiftXFormField,
) -> SiftXResult {
    if fields.is_null() || out.is_null() {
        return SiftXResult::InvalidArg;
    }
    let arr = unsafe { &*fields };
    if index >= arr.fields.len() {
        set_last_error("form field index out of bounds");
        return SiftXResult::InvalidArg;
    }
    unsafe {
        *out = arr.fields[index].as_c_field();
    }
    SiftXResult::Ok
}

/// Free a form field array. NULL is safely ignored.
///
/// # Safety
///
/// - `fields` must be NULL, or a `SiftXFormFieldArray*` from
///   `siftx_form_fields()`.
/// - It must not have been freed already, and neither it nor any pointer
///   obtained from it may be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_form_fields_free(fields: *mut SiftXFormFieldArray) {
    if !fields.is_null() {
        drop(unsafe { Box::from_raw(fields) });
    }
}

// ---------------------------------------------------------------------------
// PDF: Annotations
// ---------------------------------------------------------------------------

/// A single annotation (C-compatible).
#[repr(C)]
pub struct SiftXAnnotation {
    /// Annotation type name: "Text", "Link", "Highlight", etc.
    pub annot_type: *const c_char,
    /// 0-based page index.
    pub page: u32,
    /// Rectangle [llx, lly, urx, ury].
    pub rect: [f64; 4],
    /// /Contents text, or NULL.
    pub contents: *const c_char,
    /// Destination URI (Link annotations), or NULL.
    pub dest: *const c_char,
    /// Annotation flags.
    pub flags: u32,
    /// 1 if appearance stream exists, 0 otherwise.
    pub has_appearance: i32,
}

struct SiftXAnnotationOwned {
    annot_type: CString,
    page: u32,
    rect: [f64; 4],
    contents: Option<CString>,
    dest: Option<CString>,
    flags: u32,
    has_appearance: bool,
}

impl SiftXAnnotationOwned {
    fn as_c_annot(&self) -> SiftXAnnotation {
        SiftXAnnotation {
            annot_type: self.annot_type.as_ptr(),
            page: self.page,
            rect: self.rect,
            contents: self
                .contents
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(ptr::null()),
            dest: self
                .dest
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(ptr::null()),
            flags: self.flags,
            has_appearance: if self.has_appearance { 1 } else { 0 },
        }
    }
}

pub struct SiftXAnnotationArray {
    annots: Vec<SiftXAnnotationOwned>,
}

/// Extract all annotations from a PDF document.
///
/// Returns `SIFTX_OK` with an empty array for non-PDF documents.
///
/// # Safety
///
/// - `doc` must be NULL, or a `SiftXDocument*` from `siftx_parse()` or
///   `siftx_read()` that has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `*mut
///   SiftXAnnotationArray`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_annotations(
    doc: *const SiftXDocument,
    out: *mut *mut SiftXAnnotationArray,
) -> SiftXResult {
    if doc.is_null() || out.is_null() {
        set_last_error("null pointer argument");
        if !out.is_null() {
            unsafe {
                *out = ptr::null_mut();
            }
        }
        return SiftXResult::InvalidArg;
    }

    unsafe {
        *out = ptr::null_mut();
    }
    let doc_ref = unsafe { &*doc };

    #[cfg(feature = "pdf")]
    {
        match doc_ref.inner.all_annotations() {
            Ok(annots) => {
                let owned: Vec<SiftXAnnotationOwned> = annots
                    .iter()
                    .map(|a| SiftXAnnotationOwned {
                        annot_type: CString::new(a.annot_type.name()).unwrap_or_default(),
                        page: a.page_index as u32,
                        rect: a.rect,
                        contents: a
                            .contents
                            .as_ref()
                            .and_then(|s| CString::new(s.as_str()).ok()),
                        dest: a.dest.as_ref().and_then(|s| CString::new(s.as_str()).ok()),
                        flags: a.flags,
                        has_appearance: a.has_appearance,
                    })
                    .collect();
                let handle = Box::new(SiftXAnnotationArray { annots: owned });
                unsafe {
                    *out = Box::into_raw(handle);
                }
                return SiftXResult::Ok;
            }
            Err(e) => return result_from_error(&e),
        }
    }

    #[cfg(not(feature = "pdf"))]
    {
        let handle = Box::new(SiftXAnnotationArray { annots: Vec::new() });
        unsafe {
            *out = Box::into_raw(handle);
        }
        SiftXResult::Ok
    }
}

/// Number of annotations.
///
/// # Safety
///
/// - `annots` must be NULL, or a `SiftXAnnotationArray*` from
///   `siftx_annotations()` that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_annotations_count(annots: *const SiftXAnnotationArray) -> usize {
    if annots.is_null() {
        return 0;
    }
    unsafe { &*annots }.annots.len()
}

/// Get annotation at index.
///
/// # Safety
///
/// - `annots` must be NULL, or a `SiftXAnnotationArray*` from
///   `siftx_annotations()` that has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `SiftXAnnotation`.
/// - The pointers written into `*out` borrow the array. They are invalidated by
///   `siftx_annotations_free()` - copy anything you need to keep.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_annotations_get(
    annots: *const SiftXAnnotationArray,
    index: usize,
    out: *mut SiftXAnnotation,
) -> SiftXResult {
    if annots.is_null() || out.is_null() {
        return SiftXResult::InvalidArg;
    }
    let arr = unsafe { &*annots };
    if index >= arr.annots.len() {
        set_last_error("annotation index out of bounds");
        return SiftXResult::InvalidArg;
    }
    unsafe {
        *out = arr.annots[index].as_c_annot();
    }
    SiftXResult::Ok
}

/// Free an annotation array. NULL is safely ignored.
///
/// # Safety
///
/// - `annots` must be NULL, or a `SiftXAnnotationArray*` from
///   `siftx_annotations()`.
/// - It must not have been freed already, and neither it nor any pointer
///   obtained from it may be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_annotations_free(annots: *mut SiftXAnnotationArray) {
    if !annots.is_null() {
        drop(unsafe { Box::from_raw(annots) });
    }
}

// ---------------------------------------------------------------------------
// PDF: Structure tree
// ---------------------------------------------------------------------------

/// A single structure element (C-compatible, flattened depth-first).
#[repr(C)]
pub struct SiftXStructElement {
    /// Structure type: "Document", "P", "H1", "Table", etc.
    pub struct_type: *const c_char,
    /// Nesting depth (0 = root level).
    pub depth: u32,
    /// /T - title, or NULL.
    pub title: *const c_char,
    /// /Alt - alternative text, or NULL.
    pub alt_text: *const c_char,
    /// /ActualText - replacement text, or NULL.
    pub actual_text: *const c_char,
    /// /Lang - language tag, or NULL.
    pub lang: *const c_char,
}

struct SiftXStructElementOwned {
    struct_type: CString,
    depth: u32,
    title: Option<CString>,
    alt_text: Option<CString>,
    actual_text: Option<CString>,
    lang: Option<CString>,
}

impl SiftXStructElementOwned {
    fn as_c_elem(&self) -> SiftXStructElement {
        SiftXStructElement {
            struct_type: self.struct_type.as_ptr(),
            depth: self.depth,
            title: self
                .title
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(ptr::null()),
            alt_text: self
                .alt_text
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(ptr::null()),
            actual_text: self
                .actual_text
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(ptr::null()),
            lang: self
                .lang
                .as_ref()
                .map(|s| s.as_ptr())
                .unwrap_or(ptr::null()),
        }
    }
}

pub struct SiftXStructTreeArray {
    elements: Vec<SiftXStructElementOwned>,
    role_map: Vec<(CString, CString)>,
}

#[cfg(feature = "pdf")]
fn flatten_struct_tree(
    elements: &[crate::pdf::struct_tree::StructElement],
    depth: u32,
    out: &mut Vec<SiftXStructElementOwned>,
) {
    for elem in elements {
        out.push(SiftXStructElementOwned {
            struct_type: CString::new(elem.struct_type.as_str()).unwrap_or_default(),
            depth,
            title: elem
                .title
                .as_ref()
                .and_then(|s| CString::new(s.as_str()).ok()),
            alt_text: elem
                .alt_text
                .as_ref()
                .and_then(|s| CString::new(s.as_str()).ok()),
            actual_text: elem
                .actual_text
                .as_ref()
                .and_then(|s| CString::new(s.as_str()).ok()),
            lang: elem
                .lang
                .as_ref()
                .and_then(|s| CString::new(s.as_str()).ok()),
        });
        for child in &elem.children {
            if let crate::pdf::struct_tree::StructChild::Element(sub) = child {
                flatten_struct_tree(std::slice::from_ref(sub), depth + 1, out);
            }
        }
    }
}

/// Extract the tagged structure tree from a PDF document.
///
/// Returns `SIFTX_OK` with an empty array for non-tagged/non-PDF documents.
///
/// # Safety
///
/// - `doc` must be NULL, or a `SiftXDocument*` from `siftx_parse()` or
///   `siftx_read()` that has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `*mut
///   SiftXStructTreeArray`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_struct_tree(
    doc: *const SiftXDocument,
    out: *mut *mut SiftXStructTreeArray,
) -> SiftXResult {
    if doc.is_null() || out.is_null() {
        set_last_error("null pointer argument");
        if !out.is_null() {
            unsafe {
                *out = ptr::null_mut();
            }
        }
        return SiftXResult::InvalidArg;
    }

    unsafe {
        *out = ptr::null_mut();
    }
    let doc_ref = unsafe { &*doc };

    #[cfg(feature = "pdf")]
    {
        match doc_ref.inner.struct_tree() {
            Ok(Some(tree)) => {
                let mut elements = Vec::new();
                flatten_struct_tree(&tree.root_elements, 0, &mut elements);
                let role_map = tree
                    .role_map
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            CString::new(k).unwrap_or_default(),
                            CString::new(v).unwrap_or_default(),
                        )
                    })
                    .collect();
                let handle = Box::new(SiftXStructTreeArray { elements, role_map });
                unsafe {
                    *out = Box::into_raw(handle);
                }
                return SiftXResult::Ok;
            }
            Ok(None) => {
                let handle = Box::new(SiftXStructTreeArray {
                    elements: Vec::new(),
                    role_map: Vec::new(),
                });
                unsafe {
                    *out = Box::into_raw(handle);
                }
                return SiftXResult::Ok;
            }
            Err(e) => return result_from_error(&e),
        }
    }

    #[cfg(not(feature = "pdf"))]
    {
        let handle = Box::new(SiftXStructTreeArray {
            elements: Vec::new(),
            role_map: Vec::new(),
        });
        unsafe {
            *out = Box::into_raw(handle);
        }
        SiftXResult::Ok
    }
}

/// Number of structure elements.
///
/// # Safety
///
/// - `tree` must be NULL, or a `SiftXStructTreeArray*` from
///   `siftx_struct_tree()` that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_struct_tree_count(tree: *const SiftXStructTreeArray) -> usize {
    if tree.is_null() {
        return 0;
    }
    unsafe { &*tree }.elements.len()
}

/// Get structure element at index.
///
/// # Safety
///
/// - `tree` must be NULL, or a `SiftXStructTreeArray*` from
///   `siftx_struct_tree()` that has not been freed.
/// - `out` must be NULL, or point to a writable, aligned `SiftXStructElement`.
/// - The pointers written into `*out` borrow the array. They are invalidated by
///   `siftx_struct_tree_free()` - copy anything you need to keep.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_struct_tree_get(
    tree: *const SiftXStructTreeArray,
    index: usize,
    out: *mut SiftXStructElement,
) -> SiftXResult {
    if tree.is_null() || out.is_null() {
        return SiftXResult::InvalidArg;
    }
    let arr = unsafe { &*tree };
    if index >= arr.elements.len() {
        set_last_error("struct element index out of bounds");
        return SiftXResult::InvalidArg;
    }
    unsafe {
        *out = arr.elements[index].as_c_elem();
    }
    SiftXResult::Ok
}

/// Number of role map entries.
///
/// # Safety
///
/// - `tree` must be NULL, or a `SiftXStructTreeArray*` from
///   `siftx_struct_tree()` that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_struct_tree_role_map_count(
    tree: *const SiftXStructTreeArray,
) -> usize {
    if tree.is_null() {
        return 0;
    }
    unsafe { &*tree }.role_map.len()
}

/// Get role map entry at index. Returns the custom role and standard role as strings.
///
/// # Safety
///
/// - `tree` must be NULL, or a `SiftXStructTreeArray*` from
///   `siftx_struct_tree()` that has not been freed.
/// - `out_custom` must be NULL, or point to a writable, aligned `*const
///   c_char`.
/// - `out_standard` must be NULL, or point to a writable, aligned `*const
///   c_char`.
/// - The pointers written into `*out` borrow the array. They are invalidated by
///   `siftx_struct_tree_free()` - copy anything you need to keep.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_struct_tree_role_map_get(
    tree: *const SiftXStructTreeArray,
    index: usize,
    out_custom: *mut *const c_char,
    out_standard: *mut *const c_char,
) -> SiftXResult {
    if tree.is_null() || out_custom.is_null() || out_standard.is_null() {
        return SiftXResult::InvalidArg;
    }
    let arr = unsafe { &*tree };
    if index >= arr.role_map.len() {
        set_last_error("role map index out of bounds");
        return SiftXResult::InvalidArg;
    }
    unsafe {
        *out_custom = arr.role_map[index].0.as_ptr();
        *out_standard = arr.role_map[index].1.as_ptr();
    }
    SiftXResult::Ok
}

/// Free a structure tree array. NULL is safely ignored.
///
/// # Safety
///
/// - `tree` must be NULL, or a `SiftXStructTreeArray*` from
///   `siftx_struct_tree()`.
/// - It must not have been freed already, and neither it nor any pointer
///   obtained from it may be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_struct_tree_free(tree: *mut SiftXStructTreeArray) {
    if !tree.is_null() {
        drop(unsafe { Box::from_raw(tree) });
    }
}

// ---------------------------------------------------------------------------
// Document file type
// ---------------------------------------------------------------------------

/// Get the detected file type of a document.
///
/// # Safety
///
/// - `doc` must be NULL, or a `SiftXDocument*` from `siftx_parse()` or
///   `siftx_read()` that has not been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn siftx_document_file_type(doc: *const SiftXDocument) -> SiftXFileType {
    if doc.is_null() {
        return SiftXFileType::Unknown;
    }
    SiftXFileType::from(unsafe { &*doc }.inner.file_type())
}

// ---------------------------------------------------------------------------
// Version
// ---------------------------------------------------------------------------

/// Get the library version string. Returns a static NUL-terminated string.
///
/// The returned pointer is valid for the lifetime of the program. Do NOT free it.
#[unsafe(no_mangle)]
pub extern "C" fn siftx_version() -> *const c_char {
    // Include a trailing NUL in the static string.
    static VERSION: &[u8] = b"0.1.0\0";
    VERSION.as_ptr() as *const c_char
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn version_string() {
        let v = siftx_version();
        let s = unsafe { CStr::from_ptr(v) }.to_str().unwrap();
        assert_eq!(s, "0.1.0");
    }

    #[test]
    fn open_nonexistent_returns_error() {
        let path = CString::new("/nonexistent/file.jpg").unwrap();
        let mut handle: *mut SiftXFile = ptr::null_mut();
        let result = unsafe { siftx_open(path.as_ptr(), &mut handle) };
        assert_eq!(result, SiftXResult::IoError);
        assert!(handle.is_null());

        // Check error message is set
        let msg = siftx_error_message();
        assert!(!msg.is_null());
    }

    #[test]
    fn null_args_return_invalid() {
        let mut handle: *mut SiftXFile = ptr::null_mut();
        let result = unsafe { siftx_open(ptr::null(), &mut handle) };
        assert_eq!(result, SiftXResult::InvalidArg);
    }

    #[test]
    fn read_and_extract_tags() {
        // Minimal JPEG: FF D8 FF E0 ... FF D9
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x02, 0x00, 0x00, 0xFF, 0xD9];
        let mut doc: *mut SiftXDocument = ptr::null_mut();
        let result = unsafe { siftx_read(jpeg.as_ptr(), jpeg.len(), &mut doc) };
        // May succeed or fail depending on format validation - either way shouldn't crash
        if result == SiftXResult::Ok {
            assert!(!doc.is_null());

            let ft = unsafe { siftx_document_file_type(doc) };
            assert_eq!(ft, SiftXFileType::Jpeg);

            // Extract tags (empty is fine for minimal JPEG)
            let mut tags: *mut SiftXTagArray = ptr::null_mut();
            let tr = unsafe { siftx_tags(doc, &mut tags) };
            assert_eq!(tr, SiftXResult::Ok);
            assert!(!tags.is_null());

            let count = unsafe { siftx_tags_count(tags) };
            // Just verify it doesn't crash
            let _ = count;

            unsafe {
                siftx_tags_free(tags);
            }
            unsafe {
                siftx_document_free(doc);
            }
        }
    }

    #[test]
    fn free_null_is_safe() {
        unsafe {
            siftx_file_free(ptr::null_mut());
            siftx_document_free(ptr::null_mut());
            siftx_tags_free(ptr::null_mut());
            siftx_images_free(ptr::null_mut());
            siftx_text_pages_free(ptr::null_mut());
            siftx_thumbnail_free(ptr::null_mut(), 0);
            siftx_form_fields_free(ptr::null_mut());
            siftx_annotations_free(ptr::null_mut());
            siftx_struct_tree_free(ptr::null_mut());
        }
    }

    #[test]
    fn gps_null_returns_invalid() {
        let mut gps = SiftXGps {
            latitude: 0.0,
            longitude: 0.0,
            altitude: 0.0,
            has_altitude: 0,
        };
        let result = unsafe { siftx_gps(ptr::null(), &mut gps) };
        assert_eq!(result, SiftXResult::InvalidArg);
    }

    #[test]
    fn error_message_initially_null() {
        // Clear any previous error
        LAST_ERROR.with(|e| *e.borrow_mut() = None);
        let msg = siftx_error_message();
        assert!(msg.is_null());
    }
}
