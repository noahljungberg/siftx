/*
 * siftx.h - C API for the Sift document and image processing library.
 *
 * Copyright (c) 2024 Truespar. MIT OR Apache-2.0.
 *
 * Memory model:
 *   - SiftX allocates; caller frees via the matching siftx_*_free() function.
 *   - All returned pointers are owned by the caller until freed.
 *   - Strings are NUL-terminated UTF-8 (const char*).
 *   - Error details available via siftx_error_message() (thread-local).
 *
 * Thread safety:
 *   - Each handle must be used from one thread at a time.
 *   - Multiple independent handles can be used concurrently.
 *
 * Lifetime rules:
 *   - A SiftXFile must outlive any SiftXDocument created from it via siftx_parse().
 *   - A SiftXDocument created via siftx_read() is self-contained.
 *   - Tag/Image/TextPage array pointers are valid until their _free() is called.
 */

#ifndef SIFTX_H
#define SIFTX_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* -------------------------------------------------------------------------
 * Result codes
 * ------------------------------------------------------------------------- */

typedef enum {
    SIFTX_OK           = 0,
    SIFTX_INVALID_ARG  = 1,
    SIFTX_IO_ERROR     = 2,
    SIFTX_FORMAT_ERROR = 3,
    SIFTX_TRUNCATED    = 4,
    SIFTX_UNSUPPORTED  = 5,
    SIFTX_INTERNAL     = 6,
} SiftXResult;

/* -------------------------------------------------------------------------
 * File types
 * ------------------------------------------------------------------------- */

typedef enum {
    SIFTX_TYPE_UNKNOWN   = 0,
    SIFTX_TYPE_JPEG      = 1,
    SIFTX_TYPE_PNG       = 2,
    SIFTX_TYPE_GIF       = 3,
    SIFTX_TYPE_BMP       = 4,
    SIFTX_TYPE_TIFF      = 5,
    SIFTX_TYPE_WEBP      = 6,
    SIFTX_TYPE_HEIF      = 7,
    SIFTX_TYPE_PDF       = 8,
    SIFTX_TYPE_ICC       = 9,
    SIFTX_TYPE_QUICKTIME = 10,
} SiftXFileType;

/* -------------------------------------------------------------------------
 * Image formats (for extracted PDF images)
 * ------------------------------------------------------------------------- */

typedef enum {
    SIFTX_IMAGE_JPEG      = 0,
    SIFTX_IMAGE_JPEG2000  = 1,
    SIFTX_IMAGE_JBIG2     = 2,
    SIFTX_IMAGE_CCITT     = 3,
    SIFTX_IMAGE_PIXELS    = 4,
} SiftXImageFormat;

/* -------------------------------------------------------------------------
 * Opaque handles
 * ------------------------------------------------------------------------- */

typedef struct SiftXFile            SiftXFile;
typedef struct SiftXDocument        SiftXDocument;
typedef struct SiftXTagArray        SiftXTagArray;
typedef struct SiftXImageArray      SiftXImageArray;
typedef struct SiftXTextPages       SiftXTextPages;
typedef struct SiftXFormFieldArray   SiftXFormFieldArray;
typedef struct SiftXAnnotationArray SiftXAnnotationArray;
typedef struct SiftXStructTreeArray SiftXStructTreeArray;

/* -------------------------------------------------------------------------
 * Tag value types
 * ------------------------------------------------------------------------- */

/** Typed value discriminant for SiftXTag. */
typedef enum {
    SIFTX_VALUE_STRING    = 0,  /**< No typed value - read the display string. */
    SIFTX_VALUE_U8        = 1,
    SIFTX_VALUE_U16       = 2,
    SIFTX_VALUE_U32       = 3,
    SIFTX_VALUE_U64       = 4,
    SIFTX_VALUE_I8        = 5,
    SIFTX_VALUE_I16       = 6,
    SIFTX_VALUE_I32       = 7,
    SIFTX_VALUE_I64       = 8,
    SIFTX_VALUE_F32       = 9,
    SIFTX_VALUE_F64       = 10,
    SIFTX_VALUE_RATIONAL  = 11, /**< Unsigned rational (num/den). */
    SIFTX_VALUE_SRATIONAL = 12, /**< Signed rational (num/den). */
} SiftXValueType;

/* -------------------------------------------------------------------------
 * Structures
 * ------------------------------------------------------------------------- */

/**
 * A single metadata tag. Pointers valid until siftx_tags_free().
 *
 * The `value` field always contains a display-ready string.
 * For EXIF tags, the typed fields provide raw parsed values so callers
 * can access ints, floats, and rationals without parsing the display string.
 */
typedef struct {
    const char*   group;        /**< Tag group: "EXIF", "XMP", "IPTC", "ICC", "PDF", "QuickTime". */
    const char*   name;         /**< Tag name: "Make", "DateCreated", etc. */
    const char*   value;        /**< Display-ready value string. */
    uint8_t       value_type;   /**< SiftXValueType discriminant (0 = string only). */
    uint8_t       _pad[3];      /**< Padding for alignment. */
    int64_t       int_val;      /**< Integer value (widened from u8/u16/u32/u64/i8/i16/i32/i64). */
    int32_t       rational_num; /**< Rational numerator (for Rational/SRational). */
    int32_t       rational_den; /**< Rational denominator (for Rational/SRational). */
    double        float_val;    /**< Float value (f32->f64); also precomputed for rationals. */
} SiftXTag;

/** GPS coordinates in decimal degrees (WGS84). */
typedef struct {
    double latitude;     /**< Decimal degrees, negative = south. */
    double longitude;    /**< Decimal degrees, negative = west. */
    double altitude;     /**< Meters above sea level (NaN if unavailable). */
    int    has_altitude; /**< 1 if altitude is valid, 0 if not. */
} SiftXGps;

/** A single form field. Pointers valid until siftx_form_fields_free(). */
typedef struct {
    const char* field_type;    /**< "Text", "Button", "Choice", "Signature", "Unknown". */
    const char* name;          /**< Fully qualified field name. */
    const char* value;         /**< Current value, or NULL. */
    const char* default_value; /**< Default value, or NULL. */
    uint32_t    flags;         /**< Field flags (/Ff). */
    int32_t     is_read_only;  /**< 1 if read-only, 0 otherwise. */
    int32_t     is_required;   /**< 1 if required, 0 otherwise. */
} SiftXFormField;

/** A single annotation. Pointers valid until siftx_annotations_free(). */
typedef struct {
    const char* annot_type;    /**< "Text", "Link", "Highlight", etc. */
    uint32_t    page;          /**< 0-based page index. */
    double      rect[4];       /**< Rectangle [llx, lly, urx, ury]. */
    const char* contents;      /**< /Contents text, or NULL. */
    const char* dest;          /**< Destination URI (Link), or NULL. */
    uint32_t    flags;         /**< Annotation flags. */
    int32_t     has_appearance; /**< 1 if appearance stream exists. */
} SiftXAnnotation;

/** A single structure element. Pointers valid until siftx_struct_tree_free(). */
typedef struct {
    const char* struct_type;   /**< "Document", "P", "H1", "Table", etc. */
    uint32_t    depth;         /**< Nesting depth (0 = root). */
    const char* title;         /**< /T title, or NULL. */
    const char* alt_text;      /**< /Alt alternative text, or NULL. */
    const char* actual_text;   /**< /ActualText replacement text, or NULL. */
    const char* lang;          /**< /Lang language tag, or NULL. */
} SiftXStructElement;

/** A single extracted image. Pointers valid until siftx_images_free(). */
typedef struct {
    uint32_t page;       /**< 0-based page index. */
    uint32_t width;      /**< Pixel width. */
    uint32_t height;     /**< Pixel height. */
    uint8_t  bpc;        /**< Bits per component. */
    uint8_t  components; /**< Number of color components (1=gray, 3=RGB, 4=CMYK). */
    uint8_t  format;     /**< SiftXImageFormat value. */
    const uint8_t* data; /**< Image data bytes. */
    size_t   data_len;   /**< Length of data in bytes. */
} SiftXImage;

/* -------------------------------------------------------------------------
 * Error handling
 * ------------------------------------------------------------------------- */

/**
 * Get the last error message (thread-local).
 *
 * Returns NULL if no error. The pointer is valid until the next FFI call
 * on the same thread. Do NOT free it.
 */
const char* siftx_error_message(void);

/* -------------------------------------------------------------------------
 * Lifecycle: open / read / parse / free
 * ------------------------------------------------------------------------- */

/**
 * Open a file by path.
 *
 * @param path  NUL-terminated UTF-8 file path.
 * @param out   Receives the file handle on success (NULL on failure).
 * @return SIFTX_OK on success.
 *
 * Free with siftx_file_free(). The file must outlive any document
 * created from it via siftx_parse().
 */
SiftXResult siftx_open(const char* path, SiftXFile** out);

/**
 * Get the detected file type of an opened file.
 */
SiftXFileType siftx_file_type(const SiftXFile* file);

/**
 * Parse an opened file into a document.
 *
 * @param file  An opened file handle (must remain alive).
 * @param out   Receives the document handle on success.
 * @return SIFTX_OK on success.
 *
 * Free with siftx_document_free().
 */
SiftXResult siftx_parse(const SiftXFile* file, SiftXDocument** out);

/**
 * Parse a byte buffer into a document.
 *
 * The data is copied internally - the caller can free it after this returns.
 *
 * @param data      Pointer to file data.
 * @param data_len  Length in bytes.
 * @param out       Receives the document handle on success.
 * @return SIFTX_OK on success.
 *
 * Free with siftx_document_free().
 */
SiftXResult siftx_read(const uint8_t* data, size_t data_len, SiftXDocument** out);

/** Free a file handle. NULL is safely ignored. */
void siftx_file_free(SiftXFile* file);

/** Free a document handle. NULL is safely ignored. */
void siftx_document_free(SiftXDocument* doc);

/* -------------------------------------------------------------------------
 * Tags (metadata extraction)
 * ------------------------------------------------------------------------- */

/**
 * Extract all metadata tags from a document.
 *
 * @param doc  A parsed document handle.
 * @param out  Receives the tag array on success.
 * @return SIFTX_OK on success.
 */
SiftXResult siftx_tags(const SiftXDocument* doc, SiftXTagArray** out);

/**
 * Convenience: open a file and extract all tags in one call.
 *
 * @param path  NUL-terminated UTF-8 file path.
 * @param out   Receives the tag array on success.
 * @return SIFTX_OK on success.
 */
SiftXResult siftx_tags_from_path(const char* path, SiftXTagArray** out);

/** Number of tags in the array. */
size_t siftx_tags_count(const SiftXTagArray* tags);

/**
 * Get tag at index.
 *
 * @param tags   Tag array.
 * @param index  0-based index.
 * @param out    Receives the tag data (pointers valid until siftx_tags_free).
 * @return SIFTX_OK on success, SIFTX_INVALID_ARG if out of bounds.
 */
SiftXResult siftx_tags_get(const SiftXTagArray* tags, size_t index, SiftXTag* out);

/** Free a tag array. NULL is safely ignored. */
void siftx_tags_free(SiftXTagArray* tags);

/* -------------------------------------------------------------------------
 * GPS
 * ------------------------------------------------------------------------- */

/**
 * Extract GPS coordinates from a document.
 *
 * @param doc  A parsed document handle.
 * @param out  Receives GPS coordinates on success.
 * @return SIFTX_OK if GPS data found, SIFTX_UNSUPPORTED if not present.
 */
SiftXResult siftx_gps(const SiftXDocument* doc, SiftXGps* out);

/* -------------------------------------------------------------------------
 * Thumbnail
 * ------------------------------------------------------------------------- */

/**
 * Extract the EXIF thumbnail (IFD1 JPEG) from a document.
 *
 * @param doc       A parsed document handle.
 * @param out_data  Receives pointer to JPEG data.
 * @param out_len   Receives data length in bytes.
 * @return SIFTX_OK on success, SIFTX_UNSUPPORTED if no thumbnail.
 *
 * Free with siftx_thumbnail_free(out_data, out_len).
 */
SiftXResult siftx_thumbnail(const SiftXDocument* doc, const uint8_t** out_data, size_t* out_len);

/** Free thumbnail data. NULL is safely ignored. */
void siftx_thumbnail_free(uint8_t* data, size_t len);

/* -------------------------------------------------------------------------
 * PDF: Image extraction
 * ------------------------------------------------------------------------- */

/**
 * Extract all images from a PDF document.
 *
 * Returns SIFTX_OK with an empty array for non-PDF documents.
 *
 * @param doc  A parsed document handle.
 * @param out  Receives the image array on success.
 * @return SIFTX_OK on success.
 */
SiftXResult siftx_images(const SiftXDocument* doc, SiftXImageArray** out);

/** Number of images in the array. */
size_t siftx_images_count(const SiftXImageArray* images);

/**
 * Get image at index.
 *
 * @param images  Image array.
 * @param index   0-based index.
 * @param out     Receives image metadata (data pointer valid until siftx_images_free).
 * @return SIFTX_OK on success.
 */
SiftXResult siftx_images_get(const SiftXImageArray* images, size_t index, SiftXImage* out);

/** Free an image array. NULL is safely ignored. */
void siftx_images_free(SiftXImageArray* images);

/* -------------------------------------------------------------------------
 * PDF: Text extraction
 * ------------------------------------------------------------------------- */

/**
 * Extract text from all pages of a PDF document.
 *
 * Returns SIFTX_OK with an empty array for non-PDF documents.
 *
 * @param doc  A parsed document handle.
 * @param out  Receives the text pages array on success.
 * @return SIFTX_OK on success.
 */
SiftXResult siftx_text_pages(const SiftXDocument* doc, SiftXTextPages** out);

/** Number of text pages. */
size_t siftx_text_pages_count(const SiftXTextPages* pages);

/**
 * Get text of page at index.
 *
 * @param pages  Text pages array.
 * @param index  0-based page index.
 * @return NUL-terminated UTF-8 string, or NULL if out of bounds.
 *         Valid until siftx_text_pages_free().
 */
const char* siftx_text_pages_get(const SiftXTextPages* pages, size_t index);

/** Free a text pages array. NULL is safely ignored. */
void siftx_text_pages_free(SiftXTextPages* pages);

/* -------------------------------------------------------------------------
 * Filtered tags
 * ------------------------------------------------------------------------- */

/** Extract only EXIF tags. Same iteration/free as siftx_tags(). */
SiftXResult siftx_exif_tags(const SiftXDocument* doc, SiftXTagArray** out);

/** Extract only XMP tags. */
SiftXResult siftx_xmp_tags(const SiftXDocument* doc, SiftXTagArray** out);

/** Extract only IPTC tags. */
SiftXResult siftx_iptc_tags(const SiftXDocument* doc, SiftXTagArray** out);

/* -------------------------------------------------------------------------
 * PDF: Raw text extraction
 * ------------------------------------------------------------------------- */

/**
 * Extract raw text (no layout) from all pages of a PDF.
 * Faster than siftx_text_pages() but may lose whitespace structure.
 * Same iteration/free as siftx_text_pages().
 */
SiftXResult siftx_text_pages_raw(const SiftXDocument* doc, SiftXTextPages** out);

/* -------------------------------------------------------------------------
 * PDF: Authentication
 * ------------------------------------------------------------------------- */

/**
 * Authenticate an encrypted PDF with a password.
 *
 * @param doc          A parsed document handle.
 * @param password     Password bytes (not NUL-terminated).
 * @param password_len Password length in bytes.
 * @return 1 if accepted, 0 if wrong or not encrypted.
 */
int siftx_authenticate(SiftXDocument* doc, const uint8_t* password, size_t password_len);

/* -------------------------------------------------------------------------
 * PDF: Form fields
 * ------------------------------------------------------------------------- */

/**
 * Extract form fields from a PDF document.
 * Returns SIFTX_OK with an empty array for non-PDF/non-form documents.
 */
SiftXResult siftx_form_fields(const SiftXDocument* doc, SiftXFormFieldArray** out);

/** Number of form fields. */
size_t siftx_form_fields_count(const SiftXFormFieldArray* fields);

/**
 * Get form field at index.
 * Pointers valid until siftx_form_fields_free().
 */
SiftXResult siftx_form_fields_get(const SiftXFormFieldArray* fields, size_t index, SiftXFormField* out);

/** Free a form field array. NULL is safely ignored. */
void siftx_form_fields_free(SiftXFormFieldArray* fields);

/* -------------------------------------------------------------------------
 * PDF: Annotations
 * ------------------------------------------------------------------------- */

/**
 * Extract all annotations from a PDF document.
 * Returns SIFTX_OK with an empty array for non-PDF documents.
 */
SiftXResult siftx_annotations(const SiftXDocument* doc, SiftXAnnotationArray** out);

/** Number of annotations. */
size_t siftx_annotations_count(const SiftXAnnotationArray* annots);

/**
 * Get annotation at index.
 * Pointers valid until siftx_annotations_free().
 */
SiftXResult siftx_annotations_get(const SiftXAnnotationArray* annots, size_t index, SiftXAnnotation* out);

/** Free an annotation array. NULL is safely ignored. */
void siftx_annotations_free(SiftXAnnotationArray* annots);

/* -------------------------------------------------------------------------
 * PDF: Structure tree
 * ------------------------------------------------------------------------- */

/**
 * Extract the tagged structure tree from a PDF.
 * Elements are flattened depth-first with depth markers.
 * Returns SIFTX_OK with an empty array for non-tagged/non-PDF documents.
 */
SiftXResult siftx_struct_tree(const SiftXDocument* doc, SiftXStructTreeArray** out);

/** Number of structure elements. */
size_t siftx_struct_tree_count(const SiftXStructTreeArray* tree);

/** Get structure element at index. */
SiftXResult siftx_struct_tree_get(const SiftXStructTreeArray* tree, size_t index, SiftXStructElement* out);

/** Number of role map entries. */
size_t siftx_struct_tree_role_map_count(const SiftXStructTreeArray* tree);

/** Get role map entry at index. */
SiftXResult siftx_struct_tree_role_map_get(const SiftXStructTreeArray* tree, size_t index,
                                          const char** out_custom, const char** out_standard);

/** Free a structure tree array. NULL is safely ignored. */
void siftx_struct_tree_free(SiftXStructTreeArray* tree);

/* -------------------------------------------------------------------------
 * Document info
 * ------------------------------------------------------------------------- */

/** Get the detected file type of a document. */
SiftXFileType siftx_document_file_type(const SiftXDocument* doc);

/* -------------------------------------------------------------------------
 * Library info
 * ------------------------------------------------------------------------- */

/** Get the library version string. Do NOT free the returned pointer. */
const char* siftx_version(void);

#ifdef __cplusplus
}
#endif

#endif /* SIFTX_H */
