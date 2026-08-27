//! PDF image extraction (IE1-IE13).
//!
//! Extracts embedded images from PDF pages with lossless passthrough:
//! JPEG, JPEG2000, JBIG2, and CCITT fax data are extracted bit-for-bit
//! without re-encoding. FlateDecode/LZW images are decompressed to raw pixels.
//!
//! Modeled after `pdfimages` - supports passthrough, listing, and dedup.

use std::collections::HashSet;

use super::content::ContentInterpreter;
use super::decode;
use super::document::{Document, Page};
use super::object::{PdfObject, Ref};
use crate::core::{Error, Result};

// ---------------------------------------------------------------------------
// IE2: Image types and metadata
// ---------------------------------------------------------------------------

/// Color space of an extracted image.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageColorSpace {
    DeviceGray,
    DeviceRGB,
    DeviceCMYK,
    CalGray,
    CalRGB,
    ICCBased {
        components: u8,
    },
    Indexed {
        base: Box<ImageColorSpace>,
        num_colors: u32,
    },
    Separation,
    DeviceN,
    Unknown,
}

/// Original encoding of the image in the PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageEncoding {
    /// DCTDecode - JPEG.
    Jpeg,
    /// JPXDecode - JPEG2000.
    Jpeg2000,
    /// JBIG2Decode.
    Jbig2,
    /// CCITTFaxDecode.
    Ccitt,
    /// FlateDecode - decompressed to raw pixels.
    Flate,
    /// LZWDecode - decompressed to raw pixels.
    Lzw,
    /// RunLengthDecode.
    RunLength,
    /// No filter - uncompressed pixel data.
    Raw,
}

/// Image type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageType {
    /// Regular image with color data.
    Image,
    /// 1-bit stencil mask (/ImageMask true).
    Stencil,
    /// Soft mask (alpha channel from /SMask).
    SoftMask,
    /// Hard mask (/Mask referencing an image XObject).
    Mask,
}

/// CCITT fax encoding type (from /K parameter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CcittEncoding {
    /// K = 0: Group 3, 1-D (modified Huffman).
    Group3_1D,
    /// K > 0: Group 3, mixed 1-D/2-D.
    Group3_2D,
    /// K < 0: Group 4 (2-D MMR).
    Group4,
}

/// CCITT fax decoding parameters.
#[derive(Debug, Clone)]
pub struct CcittParams {
    /// Encoding type derived from /K parameter.
    pub encoding: CcittEncoding,
    /// Image width in pixels (/Columns).
    pub columns: u32,
    /// Image height in pixels (/Rows, or from /Height).
    pub rows: u32,
    /// Whether end-of-line codes are present.
    pub end_of_line: bool,
    /// Whether 1-bits represent black (vs white).
    pub black_is_1: bool,
    /// Whether byte boundaries are aligned.
    pub encoded_byte_align: bool,
}

/// The extracted image data - format depends on encoding.
#[derive(Debug, Clone)]
pub enum ImageData {
    /// Raw passthrough bytes - bit-identical to the embedded stream.
    /// For JPEG: complete JPEG file (SOI through EOI).
    /// For JP2: complete JPEG2000 codestream.
    Passthrough(Vec<u8>),

    /// JBIG2: page-specific data + optional shared globals.
    Jbig2 {
        /// Page-specific JBIG2 segments.
        page_data: Vec<u8>,
        /// Optional shared symbol dictionaries (/JBIG2Globals).
        globals: Option<Vec<u8>>,
    },

    /// CCITT fax: raw bitstream + decoding parameters.
    Ccitt {
        /// Raw CCITT-encoded data.
        data: Vec<u8>,
        /// Parameters needed for decoding.
        params: CcittParams,
    },

    /// Decoded pixel data (row-major, components interleaved).
    Pixels(Vec<u8>),

    /// No data (list mode - metadata only).
    Empty,
}

/// An extracted image with metadata and data.
#[derive(Debug, Clone)]
pub struct PdfImage {
    /// Image index (global counter across all pages).
    pub index: u32,
    /// Source page (0-based).
    pub page: u32,
    /// PDF object reference - None for inline images.
    pub obj_ref: Option<(u32, u16)>,
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Bits per component (1, 2, 4, 8, 16).
    pub bpc: u8,
    /// Color space.
    pub color_space: ImageColorSpace,
    /// Number of color components.
    pub components: u8,
    /// Original encoding in the PDF.
    pub encoding: ImageEncoding,
    /// Image type (regular, stencil, soft mask).
    pub image_type: ImageType,
    /// Interpolation flag.
    pub interpolate: bool,
    /// The extracted image data.
    pub data: ImageData,
}

// ---------------------------------------------------------------------------
// IE1: Find image XObjects on a page
// ---------------------------------------------------------------------------

/// IE1: Find all image XObject references in a page's resources.
///
/// Returns (resource_name, resolved_image_object, optional_obj_ref) for each.
fn find_image_xobjects<'a>(
    doc: &'a Document<'a>,
    resources: Option<&PdfObject>,
) -> Vec<(Vec<u8>, PdfObject, Option<Ref>)> {
    let mut images = Vec::new();

    let resources = match resources {
        Some(r) => match doc.resolve_obj(r) {
            Ok(resolved) => resolved,
            Err(_) => return images,
        },
        None => return images,
    };

    let xobject_dict = match resources.dict_get(b"XObject") {
        Some(xo) => match doc.resolve_obj(xo) {
            Ok(resolved) => resolved,
            Err(_) => return images,
        },
        None => return images,
    };

    if let Some(entries) = xobject_dict.as_dict() {
        for (name, val) in entries {
            // Get the original ref for deduplication
            let obj_ref = val.as_ref();

            if let Ok(resolved) = doc.resolve_obj(val) {
                // Check /Subtype /Image
                let subtype = resolved
                    .dict_get(b"Subtype")
                    .and_then(|s| s.as_name_str())
                    .unwrap_or("");

                if subtype == "Image" {
                    images.push((name.clone(), resolved, obj_ref));
                }
            }
        }
    }

    images
}

// ---------------------------------------------------------------------------
// IE2: Extract image properties
// ---------------------------------------------------------------------------

/// IE2: Parse image properties from an image XObject dictionary.
fn parse_image_properties(
    img_obj: &PdfObject,
) -> (
    u32,
    u32,
    u8,
    ImageColorSpace,
    u8,
    ImageEncoding,
    ImageType,
    bool,
) {
    parse_image_properties_with_doc(None, img_obj)
}

fn parse_image_properties_with_doc(
    doc: Option<&Document>,
    img_obj: &PdfObject,
) -> (
    u32,
    u32,
    u8,
    ImageColorSpace,
    u8,
    ImageEncoding,
    ImageType,
    bool,
) {
    let width = img_obj
        .dict_get(b"Width")
        .or_else(|| img_obj.dict_get(b"W"))
        .and_then(|v| v.as_int())
        .unwrap_or(0) as u32;

    let height = img_obj
        .dict_get(b"Height")
        .or_else(|| img_obj.dict_get(b"H"))
        .and_then(|v| v.as_int())
        .unwrap_or(0) as u32;

    // IE9: Check if this is a stencil mask
    let is_mask = img_obj
        .dict_get(b"ImageMask")
        .or_else(|| img_obj.dict_get(b"IM"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let image_type = if is_mask {
        ImageType::Stencil
    } else {
        ImageType::Image
    };

    let bpc = if is_mask {
        1
    } else {
        img_obj
            .dict_get(b"BitsPerComponent")
            .or_else(|| img_obj.dict_get(b"BPC"))
            .and_then(|v| v.as_int())
            .unwrap_or(8) as u8
    };

    // IE11: Color space resolution
    let (color_space, components) = if is_mask {
        (ImageColorSpace::DeviceGray, 1)
    } else {
        parse_color_space_with_doc(doc, img_obj)
    };

    let encoding = parse_encoding(img_obj);

    let interpolate = img_obj
        .dict_get(b"Interpolate")
        .or_else(|| img_obj.dict_get(b"I"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // For JPXDecode (JPEG2000), read component count and BPC from the JP2 header.
    // Per ISO 32000, ColorSpace may be omitted for JPX, and the JP2 header
    // defines the actual bit depth (which may differ from PDF's /BitsPerComponent).
    let (color_space, components, bpc) = if encoding == ImageEncoding::Jpeg2000 {
        if let Some(doc) = doc {
            if let Some((n, jpx_bpc)) = jpx_stream_info(doc, img_obj) {
                // For Indexed colorspaces, PDF's BPC is the index width, not the data
                // depth - use the JP2 header BPC. For others, prefer PDF's explicit BPC.
                let use_jpx_bpc = matches!(color_space, ImageColorSpace::Indexed { .. })
                    || img_obj.dict_get(b"BitsPerComponent").is_none();
                let effective_bpc = if use_jpx_bpc { jpx_bpc } else { bpc };

                if matches!(color_space, ImageColorSpace::Unknown) {
                    let (cs, comp) = match n {
                        1 => (ImageColorSpace::DeviceGray, 1),
                        3 => (ImageColorSpace::DeviceRGB, 3),
                        4 => (ImageColorSpace::DeviceCMYK, 4),
                        _ => (ImageColorSpace::Unknown, n),
                    };
                    (cs, comp, effective_bpc)
                } else {
                    (color_space, components, effective_bpc)
                }
            } else {
                (color_space, components, bpc)
            }
        } else {
            (color_space, components, bpc)
        }
    } else {
        (color_space, components, bpc)
    };

    (
        width,
        height,
        bpc,
        color_space,
        components,
        encoding,
        image_type,
        interpolate,
    )
}

/// Try to determine component count and BPC from a JPEG2000 stream's SIZ marker.
/// Returns (component_count, bpc).
fn jpx_stream_info(_doc: &Document, img_obj: &PdfObject) -> Option<(u8, u8)> {
    let raw = img_obj.stream_data()?;
    let data = decode::decode_stream(img_obj, raw)
        .ok()
        .or_else(|| Some(raw.to_vec()))?;
    // JPEG2000 codestream: look for SIZ marker (0xFF51)
    // JP2 file format: starts with 0x0000000C 6A502020
    if data.len() < 4 {
        return None;
    }
    let mut pos = 0;
    // Skip JP2 file format boxes to find codestream
    if data.len() >= 12 && &data[4..8] == b"jP  " {
        // JP2 file format - find jp2c (contiguous codestream) box
        while pos + 8 <= data.len() {
            let box_len =
                u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                    as usize;
            let box_type = &data[pos + 4..pos + 8];
            if box_type == b"jp2c" {
                pos += 8;
                break;
            }
            if box_len < 8 {
                break;
            }
            pos += box_len;
        }
    }
    // Now at codestream start - look for SOC (0xFF4F) then SIZ (0xFF51)
    if pos + 2 <= data.len() && data[pos] == 0xFF && data[pos + 1] == 0x4F {
        pos += 2; // Skip SOC
        if pos + 2 <= data.len() && data[pos] == 0xFF && data[pos + 1] == 0x51 {
            pos += 2; // Skip SIZ marker
            if pos + 2 <= data.len() {
                let _lsiz = u16::from_be_bytes([data[pos], data[pos + 1]]);
                pos += 2; // Lsiz
                pos += 2; // Rsiz
                pos += 4; // Xsiz
                pos += 4; // Ysiz
                pos += 4; // XOsiz
                pos += 4; // YOsiz
                pos += 4; // XTsiz
                pos += 4; // YTsiz
                pos += 4; // XTOsiz
                pos += 4; // YTOsiz
                if pos + 2 <= data.len() {
                    let csiz = u16::from_be_bytes([data[pos], data[pos + 1]]);
                    pos += 2; // Csiz
                    // Read first component's Ssiz for BPC
                    // Ssiz: bit 7 = signed, bits 0-6 = precision - 1
                    let bpc = if pos < data.len() {
                        (data[pos] & 0x7F) + 1
                    } else {
                        8 // default
                    };
                    return Some((csiz as u8, bpc));
                }
            }
        }
    }
    None
}

/// IE11: Parse color space from image dictionary.
#[allow(dead_code)] // Convenience wrapper; today all callers go through `_with_doc`.
fn parse_color_space(img_obj: &PdfObject) -> (ImageColorSpace, u8) {
    parse_color_space_with_doc(None, img_obj)
}

/// IE11: Parse color space, resolving indirect references via doc.
fn parse_color_space_with_doc(
    doc: Option<&Document>,
    img_obj: &PdfObject,
) -> (ImageColorSpace, u8) {
    let cs = match img_obj
        .dict_get(b"ColorSpace")
        .or_else(|| img_obj.dict_get(b"CS"))
    {
        Some(cs) => cs,
        None => return (ImageColorSpace::Unknown, 1),
    };

    // Resolve indirect reference on the ColorSpace itself
    let cs = if let (Some(d), PdfObject::Ref(_)) = (doc, cs) {
        match d.resolve_obj(cs) {
            Ok(resolved) => resolved,
            Err(_) => return (ImageColorSpace::Unknown, 1),
        }
    } else {
        cs.clone()
    };

    match &cs {
        PdfObject::Name(name) => match name.as_slice() {
            b"DeviceGray" | b"G" => (ImageColorSpace::DeviceGray, 1),
            b"DeviceRGB" | b"RGB" => (ImageColorSpace::DeviceRGB, 3),
            b"DeviceCMYK" | b"CMYK" => (ImageColorSpace::DeviceCMYK, 4),
            b"CalGray" => (ImageColorSpace::CalGray, 1),
            b"CalRGB" => (ImageColorSpace::CalRGB, 3),
            _ => (ImageColorSpace::Unknown, 1),
        },
        PdfObject::Array(arr) if !arr.is_empty() => {
            let cs_name = arr[0].as_name_str().unwrap_or("");
            match cs_name {
                "DeviceGray" => (ImageColorSpace::DeviceGray, 1),
                "DeviceRGB" => (ImageColorSpace::DeviceRGB, 3),
                "DeviceCMYK" => (ImageColorSpace::DeviceCMYK, 4),
                "CalGray" => (ImageColorSpace::CalGray, 1),
                "CalRGB" => (ImageColorSpace::CalRGB, 3),
                "ICCBased" => {
                    // [/ICCBased stream_ref] - components from /N in stream dict
                    let n = if arr.len() > 1 {
                        // Resolve indirect reference to get the stream dict
                        let icc_obj = if let Some(d) = doc {
                            d.resolve_obj(&arr[1]).ok()
                        } else {
                            None
                        };
                        let target = icc_obj.as_ref().unwrap_or(&arr[1]);
                        target.dict_get(b"N").and_then(|v| v.as_int()).unwrap_or(3) as u8
                    } else {
                        3
                    };
                    (ImageColorSpace::ICCBased { components: n }, n)
                }
                "Indexed" | "I" => {
                    // [/Indexed base hival lookup]
                    let (base, _base_comp) = if arr.len() > 1 {
                        parse_color_space_obj(&arr[1])
                    } else {
                        (ImageColorSpace::DeviceRGB, 3)
                    };
                    let hival = if arr.len() > 2 {
                        arr[2].as_int().unwrap_or(255) as u32
                    } else {
                        255
                    };
                    // Indexed images have 1 component (the index)
                    (
                        ImageColorSpace::Indexed {
                            base: Box::new(base),
                            num_colors: hival + 1,
                        },
                        1,
                    )
                    // Note: base_comp is used for palette interpretation, not pixel data
                }
                "Separation" => (ImageColorSpace::Separation, 1),
                "DeviceN" => {
                    let n = if arr.len() > 1 {
                        arr[1].as_array().map(|a| a.len()).unwrap_or(1) as u8
                    } else {
                        1
                    };
                    (ImageColorSpace::DeviceN, n)
                }
                _ => (ImageColorSpace::Unknown, 1),
            }
        }
        _ => (ImageColorSpace::Unknown, 1),
    }
}

/// Helper: parse a color space from a standalone PdfObject.
fn parse_color_space_obj(obj: &PdfObject) -> (ImageColorSpace, u8) {
    match obj {
        PdfObject::Name(name) => match name.as_slice() {
            b"DeviceGray" | b"G" => (ImageColorSpace::DeviceGray, 1),
            b"DeviceRGB" | b"RGB" => (ImageColorSpace::DeviceRGB, 3),
            b"DeviceCMYK" | b"CMYK" => (ImageColorSpace::DeviceCMYK, 4),
            _ => (ImageColorSpace::Unknown, 1),
        },
        _ => (ImageColorSpace::Unknown, 1),
    }
}

/// Determine the image encoding from /Filter.
fn parse_encoding(img_obj: &PdfObject) -> ImageEncoding {
    let filters = decode::get_filters(img_obj);
    if filters.is_empty() {
        return ImageEncoding::Raw;
    }

    // The last filter in the chain is the image-format filter
    match filters.last().unwrap().as_slice() {
        b"DCTDecode" | b"DCT" => ImageEncoding::Jpeg,
        b"JPXDecode" => ImageEncoding::Jpeg2000,
        b"JBIG2Decode" => ImageEncoding::Jbig2,
        b"CCITTFaxDecode" | b"CCF" => ImageEncoding::Ccitt,
        b"FlateDecode" | b"Fl" => ImageEncoding::Flate,
        b"LZWDecode" | b"LZW" => ImageEncoding::Lzw,
        b"RunLengthDecode" | b"RL" => ImageEncoding::RunLength,
        _ => ImageEncoding::Raw,
    }
}

/// Check whether the last filter is an image-format filter (passthrough-eligible).
fn is_passthrough_filter(filter: &[u8]) -> bool {
    matches!(
        filter,
        b"DCTDecode" | b"DCT" | b"JPXDecode" | b"JBIG2Decode" | b"CCITTFaxDecode" | b"CCF"
    )
}

// ---------------------------------------------------------------------------
// IE3-IE7: Extract image data (passthrough or decode)
// ---------------------------------------------------------------------------

/// Extract image data from a resolved image XObject.
///
/// For JPEG/JP2/JBIG2/CCITT: passthrough (bit-identical extraction).
/// For Flate/LZW/Raw: decode to pixel data.
fn extract_image_data(
    doc: &Document,
    img_obj: &PdfObject,
    encoding: ImageEncoding,
    height: u32,
) -> Result<ImageData> {
    let raw = img_obj
        .stream_data()
        .ok_or_else(|| Error::Format("image has no stream data".into()))?;

    let filters = decode::get_filters(img_obj);

    match encoding {
        // IE3: JPEG passthrough
        ImageEncoding::Jpeg => {
            let data = if filters.len() > 1 && is_passthrough_filter(filters.last().unwrap()) {
                // Multi-filter chain: apply all except the last (image) filter
                decode::decode_stream_except_last(img_obj, raw)?
            } else {
                // Single DCTDecode: raw bytes ARE the JPEG
                raw.to_vec()
            };
            Ok(ImageData::Passthrough(data))
        }

        // IE5: JPEG2000 passthrough
        ImageEncoding::Jpeg2000 => {
            let data = if filters.len() > 1 && is_passthrough_filter(filters.last().unwrap()) {
                decode::decode_stream_except_last(img_obj, raw)?
            } else {
                raw.to_vec()
            };
            Ok(ImageData::Passthrough(data))
        }

        // IE7: JBIG2 passthrough
        ImageEncoding::Jbig2 => {
            let page_data = if filters.len() > 1 && is_passthrough_filter(filters.last().unwrap()) {
                decode::decode_stream_except_last(img_obj, raw)?
            } else {
                raw.to_vec()
            };

            // Extract globals from /DecodeParms -> /JBIG2Globals
            let globals = extract_jbig2_globals(doc, img_obj);

            Ok(ImageData::Jbig2 { page_data, globals })
        }

        // IE6: CCITT passthrough
        ImageEncoding::Ccitt => {
            let data = if filters.len() > 1 && is_passthrough_filter(filters.last().unwrap()) {
                decode::decode_stream_except_last(img_obj, raw)?
            } else {
                raw.to_vec()
            };

            let params = extract_ccitt_params(img_obj, height);

            Ok(ImageData::Ccitt { data, params })
        }

        // IE4: FlateDecode / LZW / RunLength / Raw -> decode to pixels
        ImageEncoding::Flate | ImageEncoding::Lzw | ImageEncoding::RunLength => {
            let decoded = decode::decode_stream(img_obj, raw)?;
            Ok(ImageData::Pixels(decoded))
        }

        ImageEncoding::Raw => Ok(ImageData::Pixels(raw.to_vec())),
    }
}

/// IE7: Extract JBIG2 global segments from /DecodeParms -> /JBIG2Globals.
fn extract_jbig2_globals(doc: &Document, img_obj: &PdfObject) -> Option<Vec<u8>> {
    let filters = decode::get_filters(img_obj);
    let parms_list = decode::get_decode_parms(img_obj, filters.len());

    // Find the JBIG2 filter's decode parms (the last filter)
    let jbig2_parms = parms_list.last()?.as_ref()?;

    let globals_ref = jbig2_parms.dict_get(b"JBIG2Globals")?;
    let globals_stream = doc.resolve_obj(globals_ref).ok()?;
    let globals_raw = globals_stream.stream_data()?;

    // Decode any transport filters on the globals stream
    decode::decode_stream(&globals_stream, globals_raw).ok()
}

/// IE6: Extract CCITT fax decoding parameters.
fn extract_ccitt_params(img_obj: &PdfObject, default_rows: u32) -> CcittParams {
    let filters = decode::get_filters(img_obj);
    let parms_list = decode::get_decode_parms(img_obj, filters.len());

    // Find the CCITT filter's decode parms (the last filter)
    let parms = parms_list.last().and_then(|p| p.as_ref());

    let k = parms
        .and_then(|p| p.dict_get(b"K"))
        .and_then(|v| v.as_int())
        .unwrap_or(0);

    let encoding = if k < 0 {
        CcittEncoding::Group4
    } else if k == 0 {
        CcittEncoding::Group3_1D
    } else {
        CcittEncoding::Group3_2D
    };

    let columns = parms
        .and_then(|p| p.dict_get(b"Columns"))
        .and_then(|v| v.as_int())
        .unwrap_or(1728) as u32;

    let rows = parms
        .and_then(|p| p.dict_get(b"Rows"))
        .and_then(|v| v.as_int())
        .map(|v| v as u32)
        .unwrap_or(default_rows);

    let end_of_line = parms
        .and_then(|p| p.dict_get(b"EndOfLine"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let black_is_1 = parms
        .and_then(|p| p.dict_get(b"BlackIs1"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let encoded_byte_align = parms
        .and_then(|p| p.dict_get(b"EncodedByteAlign"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    CcittParams {
        encoding,
        columns,
        rows,
        end_of_line,
        black_is_1,
        encoded_byte_align,
    }
}

// ---------------------------------------------------------------------------
// IE10: Soft mask extraction
// ---------------------------------------------------------------------------

/// IE10: Extract the soft mask (/SMask) for an image, if present.
fn extract_soft_mask(
    doc: &Document,
    img_obj: &PdfObject,
    page_num: u32,
    img_index: &mut u32,
) -> Option<PdfImage> {
    let smask_ref = img_obj.dict_get(b"SMask")?;
    let smask_obj = doc.resolve_obj(smask_ref).ok()?;

    // SMask is always a grayscale image
    let width = smask_obj
        .dict_get(b"Width")
        .and_then(|v| v.as_int())
        .unwrap_or(0) as u32;
    let height = smask_obj
        .dict_get(b"Height")
        .and_then(|v| v.as_int())
        .unwrap_or(0) as u32;
    let bpc = smask_obj
        .dict_get(b"BitsPerComponent")
        .and_then(|v| v.as_int())
        .unwrap_or(8) as u8;

    let encoding = parse_encoding(&smask_obj);
    let data = extract_image_data(doc, &smask_obj, encoding, height).unwrap_or(ImageData::Empty);

    let obj_ref = smask_ref.as_ref().map(|r| (r.num, r.generation));

    *img_index += 1;

    Some(PdfImage {
        index: *img_index - 1,
        page: page_num,
        obj_ref,
        width,
        height,
        bpc,
        color_space: ImageColorSpace::DeviceGray,
        components: 1,
        encoding,
        image_type: ImageType::SoftMask,
        interpolate: false,
        data,
    })
}

/// Extract hard mask (/Mask referencing an image XObject) as a separate entry.
/// Poppler lists these as type "mask" in pdfimages output.
fn extract_hard_mask(
    doc: &Document,
    img_obj: &PdfObject,
    page_num: u32,
    img_index: &mut u32,
) -> Option<PdfImage> {
    let mask_val = img_obj.dict_get(b"Mask")?;
    // /Mask can be an array (color key masking) or a reference to an image XObject
    // Only handle the image XObject case
    if mask_val.as_array().is_some() {
        return None; // Color key masking, not an image mask
    }
    let mask_obj = doc.resolve_obj(mask_val).ok()?;
    // Verify it's an image
    let subtype = mask_obj
        .dict_get(b"Subtype")
        .and_then(|s| s.as_name_str())?;
    if subtype != "Image" {
        return None;
    }

    let width = mask_obj
        .dict_get(b"Width")
        .and_then(|v| v.as_int())
        .unwrap_or(0) as u32;
    let height = mask_obj
        .dict_get(b"Height")
        .and_then(|v| v.as_int())
        .unwrap_or(0) as u32;
    let bpc = mask_obj
        .dict_get(b"BitsPerComponent")
        .and_then(|v| v.as_int())
        .unwrap_or(1) as u8;

    let is_image_mask = mask_obj
        .dict_get(b"ImageMask")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let encoding = parse_encoding(&mask_obj);
    let data = extract_image_data(doc, &mask_obj, encoding, height).unwrap_or(ImageData::Empty);
    let obj_ref = mask_val.as_ref().map(|r| (r.num, r.generation));

    // Poppler reports /ImageMask true masks as "mask" (stencil-like),
    // but regular images used as /Mask are treated as "smask" (soft mask).
    let image_type = if is_image_mask {
        ImageType::Mask
    } else {
        ImageType::SoftMask
    };

    *img_index += 1;
    Some(PdfImage {
        index: *img_index - 1,
        page: page_num,
        obj_ref,
        width,
        height,
        bpc,
        color_space: ImageColorSpace::DeviceGray,
        components: 1,
        encoding,
        image_type,
        interpolate: false,
        data,
    })
}

// ---------------------------------------------------------------------------
// IE12: List mode + IE1-IE13: Full extraction
// ---------------------------------------------------------------------------

/// IE1-IE11: Extract all images from a single page.
///
/// Lossless passthrough for JPEG, JPEG2000, JBIG2, and CCITT.
/// Decompresses Flate/LZW to raw pixels.
pub fn extract_images(
    doc: &Document,
    page: &Page,
    page_num: u32,
    img_counter: &mut u32,
    seen_refs: &mut HashSet<(u32, u16)>,
) -> Result<Vec<PdfImage>> {
    let mut results = Vec::new();

    // Build full XObject map (Image + Form) from page resources
    let xobject_map = build_full_xobject_map(doc, page.resources.as_ref());

    // Process content stream operators (Do and gs) in order for correct image ordering.
    if let Ok(content_data) = get_page_content(doc, page) {
        if !content_data.is_empty() {
            extract_content_stream_images(
                doc,
                &content_data,
                page.resources.as_ref(),
                &xobject_map,
                page_num,
                img_counter,
                seen_refs,
                &mut results,
                0,
            );
        }
    }

    // Process annotation appearance streams (/Annots -> /AP -> /N)
    extract_annotation_images(doc, page, page_num, img_counter, seen_refs, &mut results);

    // Fallback: if content stream is missing, use resource dict enumeration.
    let has_content_stream = page.dict.dict_get(b"Contents").is_some();
    if results.is_empty() && !has_content_stream {
        let xobjects = find_image_xobjects(doc, page.resources.as_ref());
        for (_name, img_obj, obj_ref) in &xobjects {
            if let Some(r) = &obj_ref {
                let key = (r.num, r.generation);
                if seen_refs.contains(&key) {
                    continue;
                }
                seen_refs.insert(key);
            }

            let (width, height, bpc, color_space, components, encoding, image_type, interpolate) =
                parse_image_properties_with_doc(Some(doc), &img_obj);
            let data = match extract_image_data(doc, &img_obj, encoding, height) {
                Ok(d) => d,
                Err(_) => continue,
            };

            let image = PdfImage {
                index: *img_counter,
                page: page_num,
                obj_ref: obj_ref.map(|r| (r.num, r.generation)),
                width,
                height,
                bpc,
                color_space,
                components,
                encoding,
                image_type,
                interpolate,
                data,
            };
            *img_counter += 1;
            results.push(image);

            if let Some(smask) = extract_soft_mask(doc, &img_obj, page_num, img_counter) {
                results.push(smask);
            }
        }
    }

    Ok(results)
}

/// Process content stream operators (Do, gs, BI) in order.
/// This ensures images are extracted in the correct content stream order,
/// interleaving Do targets and ExtGState/SMask/G images as they appear.
fn extract_content_stream_images(
    doc: &Document,
    content_data: &[u8],
    resources: Option<&PdfObject>,
    xobject_map: &std::collections::HashMap<Vec<u8>, XObjectEntry>,
    page_num: u32,
    img_counter: &mut u32,
    seen_refs: &mut HashSet<(u32, u16)>,
    results: &mut Vec<PdfImage>,
    depth: u32,
) {
    const MAX_DEPTH: u32 = 10;
    if depth >= MAX_DEPTH {
        return;
    }

    use super::content::ContentStreamOp;

    // Get all operators in content stream order (Do, gs, BI interleaved)
    let ops = ContentInterpreter::find_operators_in_order(content_data);

    // Lazily resolve ExtGState dict for gs processing
    let extgstate_dict = resources
        .and_then(|r| doc.resolve_obj(r).ok())
        .and_then(|res| res.dict_get(b"ExtGState").cloned())
        .and_then(|gs| doc.resolve_obj(&gs).ok());

    for cs_op in &ops {
        match cs_op {
            ContentStreamOp::Ref { op, name } => {
                if *op == b"Do" {
                    extract_do_targets(
                        doc,
                        &[name.clone()],
                        xobject_map,
                        page_num,
                        img_counter,
                        seen_refs,
                        results,
                        depth,
                    );
                } else if *op == b"gs" {
                    if let Some(ref gs_dict) = extgstate_dict {
                        extract_single_gs_smask(
                            doc,
                            gs_dict,
                            name,
                            page_num,
                            img_counter,
                            seen_refs,
                            results,
                            depth,
                        );
                    }
                }
            }
            ContentStreamOp::InlineImage(inline_tokens) => {
                if let Some(img) = parse_inline_image_tokens(inline_tokens, page_num, img_counter) {
                    results.push(img);
                }
            }
        }
    }
}

/// Process a single gs operator target for ExtGState/SMask/G images.
fn extract_single_gs_smask(
    doc: &Document,
    extgstate_dict: &PdfObject,
    gs_name: &[u8],
    page_num: u32,
    img_counter: &mut u32,
    seen_refs: &mut HashSet<(u32, u16)>,
    results: &mut Vec<PdfImage>,
    depth: u32,
) {
    let gs_obj = match extgstate_dict.dict_get(gs_name) {
        Some(obj) => match doc.resolve_obj(obj) {
            Ok(resolved) => resolved,
            Err(_) => return,
        },
        None => return,
    };

    let smask = match gs_obj.dict_get(b"SMask") {
        Some(sm) => match doc.resolve_obj(sm) {
            Ok(resolved) => resolved,
            Err(_) => return,
        },
        None => return,
    };

    if smask.as_name_str() == Some("None") {
        return;
    }

    let g_obj = match smask.dict_get(b"G") {
        Some(g) => match doc.resolve_obj(g) {
            Ok(resolved) => resolved,
            Err(_) => return,
        },
        None => return,
    };

    let subtype = g_obj
        .dict_get(b"Subtype")
        .and_then(|s| s.as_name_str())
        .unwrap_or("");
    if subtype != "Form" {
        return;
    }

    let raw = match g_obj.stream_data() {
        Some(r) => r,
        None => return,
    };
    let form_content = match decode::decode_stream(&g_obj, raw) {
        Ok(d) => d,
        Err(_) => return,
    };

    let form_resources = g_obj.dict_get(b"Resources");
    let form_map = build_full_xobject_map(doc, form_resources);

    extract_content_stream_images(
        doc,
        &form_content,
        form_resources,
        &form_map,
        page_num,
        img_counter,
        seen_refs,
        results,
        depth + 1,
    );
}

/// Process Do targets from a content stream, recursing into Form XObjects.
/// `depth` limits recursion to prevent infinite loops.
fn extract_do_targets(
    doc: &Document,
    do_targets: &[Vec<u8>],
    xobject_map: &std::collections::HashMap<Vec<u8>, XObjectEntry>,
    page_num: u32,
    img_counter: &mut u32,
    seen_refs: &mut HashSet<(u32, u16)>,
    results: &mut Vec<PdfImage>,
    depth: u32,
) {
    const MAX_FORM_DEPTH: u32 = 10;

    for name in do_targets {
        match xobject_map.get(name.as_slice()) {
            Some(XObjectEntry::Image(img_obj, obj_ref)) => {
                // Deduplication
                let is_dup = if let Some(r) = obj_ref {
                    let key = (r.num, r.generation);
                    !seen_refs.insert(key)
                } else {
                    false
                };

                let (
                    width,
                    height,
                    bpc,
                    color_space,
                    components,
                    encoding,
                    image_type,
                    interpolate,
                ) = parse_image_properties_with_doc(Some(doc), img_obj);

                let data = if is_dup {
                    ImageData::Empty
                } else {
                    extract_image_data(doc, img_obj, encoding, height).unwrap_or(ImageData::Empty)
                };

                let ref_tuple = obj_ref.map(|r| (r.num, r.generation));
                results.push(PdfImage {
                    index: *img_counter,
                    page: page_num,
                    obj_ref: ref_tuple,
                    width,
                    height,
                    bpc,
                    color_space,
                    components,
                    encoding,
                    image_type,
                    interpolate,
                    data,
                });
                *img_counter += 1;

                // Extract masks for every occurrence
                if let Some(smask) = extract_soft_mask(doc, img_obj, page_num, img_counter) {
                    results.push(smask);
                }
                if let Some(mask) = extract_hard_mask(doc, img_obj, page_num, img_counter) {
                    results.push(mask);
                }
            }
            Some(XObjectEntry::Form(form_obj)) if depth < MAX_FORM_DEPTH => {
                // Recurse into Form XObject's content stream
                if let Some(raw) = form_obj.stream_data() {
                    if let Ok(content_data) = decode::decode_stream(form_obj, raw) {
                        // Build XObject map from Form's own Resources (fall back to parent map)
                        let form_resources = form_obj.dict_get(b"Resources");
                        let form_map = if form_resources.is_some() {
                            build_full_xobject_map(doc, form_resources)
                        } else {
                            std::collections::HashMap::new()
                        };
                        let effective_map = if form_map.is_empty() {
                            xobject_map
                        } else {
                            &form_map
                        };

                        // Process all operators (Do, gs, BI) in content stream order
                        extract_content_stream_images(
                            doc,
                            &content_data,
                            form_resources,
                            effective_map,
                            page_num,
                            img_counter,
                            seen_refs,
                            results,
                            depth + 1,
                        );
                    }
                }
            }
            _ => continue,
        }
    }
}

/// Extract images from annotation appearance streams on a page.
/// Poppler processes /Annots -> /AP -> /N (normal appearance) as additional
/// content streams, finding images nested inside stamp annotations, etc.
fn extract_annotation_images(
    doc: &Document,
    page: &Page,
    page_num: u32,
    img_counter: &mut u32,
    seen_refs: &mut HashSet<(u32, u16)>,
    results: &mut Vec<PdfImage>,
) {
    let annots = match page.dict.dict_get(b"Annots") {
        Some(a) => match doc.resolve_obj(a) {
            Ok(resolved) => resolved,
            Err(_) => return,
        },
        None => return,
    };

    let annot_array = match annots.as_array() {
        Some(arr) => arr,
        None => return,
    };

    for annot_ref in annot_array {
        let annot = match doc.resolve_obj(annot_ref) {
            Ok(a) => a,
            Err(_) => continue,
        };

        // Get /AP (appearance dict)
        let ap = match annot.dict_get(b"AP") {
            Some(ap) => match doc.resolve_obj(ap) {
                Ok(a) => a,
                Err(_) => continue,
            },
            None => continue,
        };

        // Process /N (normal appearance) - the primary appearance stream
        if let Some(n) = ap.dict_get(b"N") {
            process_appearance_stream(doc, n, page_num, img_counter, seen_refs, results);
        }
    }
}

/// Process a single appearance stream (Form XObject), extracting images from it.
fn process_appearance_stream(
    doc: &Document,
    appearance: &PdfObject,
    page_num: u32,
    img_counter: &mut u32,
    seen_refs: &mut HashSet<(u32, u16)>,
    results: &mut Vec<PdfImage>,
) {
    let form_obj = match doc.resolve_obj(appearance) {
        Ok(obj) => obj,
        Err(_) => return,
    };

    // Must be a stream (Form XObject)
    if form_obj.stream_data().is_none() {
        return;
    }

    let raw = match form_obj.stream_data() {
        Some(r) => r,
        None => return,
    };

    let content_data = match decode::decode_stream(&form_obj, raw) {
        Ok(d) => d,
        Err(_) => return,
    };

    // Build XObject map from the Form's resources
    let form_resources = form_obj.dict_get(b"Resources");
    let form_map = build_full_xobject_map(doc, form_resources);

    extract_content_stream_images(
        doc,
        &content_data,
        form_resources,
        &form_map,
        page_num,
        img_counter,
        seen_refs,
        results,
        0,
    );
}

/// Extract images reachable through ExtGState /SMask/G Form XObjects on a page.
/// Get concatenated decoded content stream bytes for a page.
fn get_page_content(doc: &Document, page: &Page) -> Result<Vec<u8>> {
    let contents = match page.dict.dict_get(b"Contents") {
        Some(c) => c.clone(),
        None => return Ok(Vec::new()),
    };
    let contents = doc.resolve_obj(&contents)?;
    match &contents {
        PdfObject::Stream { .. } => {
            let raw = contents.stream_data().unwrap();
            decode::decode_stream(&contents, raw)
        }
        PdfObject::Array(refs) => {
            let mut data = Vec::new();
            for item in refs {
                let resolved = doc.resolve_obj(item)?;
                if let Some(raw) = resolved.stream_data() {
                    let decoded = decode::decode_stream(&resolved, raw)?;
                    if !data.is_empty() {
                        data.push(b' ');
                    }
                    data.extend_from_slice(&decoded);
                }
            }
            Ok(data)
        }
        _ => Ok(Vec::new()),
    }
}

/// Parse inline image tokens (between BI and EI) into a PdfImage.
fn parse_inline_image_tokens(
    tokens: &[super::content::CsToken],
    page_num: u32,
    img_counter: &mut u32,
) -> Option<PdfImage> {
    use super::content::CsToken;

    let mut width = 0u32;
    let mut height = 0u32;
    let mut bpc = 8u8;
    let mut color_space = ImageColorSpace::Unknown;
    let mut components = 1u8;
    let mut encoding = ImageEncoding::Raw;
    let mut is_mask = false;

    let mut i = 0;
    // Parse key/value pairs until ID operator
    while i + 1 < tokens.len() {
        if let CsToken::Operator(ref op) = tokens[i] {
            if op == b"ID" {
                i += 1;
                break;
            }
        }
        if let CsToken::Operand(ref key_obj) = tokens[i] {
            if let Some(key) = key_obj.as_name() {
                i += 1;
                if i < tokens.len() {
                    if let CsToken::Operand(ref val_obj) = tokens[i] {
                        match key {
                            b"W" | b"Width" => {
                                width = val_obj.as_int().unwrap_or(0) as u32;
                            }
                            b"H" | b"Height" => {
                                height = val_obj.as_int().unwrap_or(0) as u32;
                            }
                            b"BPC" | b"BitsPerComponent" => {
                                bpc = val_obj.as_int().unwrap_or(8) as u8;
                            }
                            b"CS" | b"ColorSpace" => {
                                if let Some(name) = val_obj.as_name() {
                                    match name {
                                        b"G" | b"DeviceGray" => {
                                            color_space = ImageColorSpace::DeviceGray;
                                            components = 1;
                                        }
                                        b"RGB" | b"DeviceRGB" => {
                                            color_space = ImageColorSpace::DeviceRGB;
                                            components = 3;
                                        }
                                        b"CMYK" | b"DeviceCMYK" => {
                                            color_space = ImageColorSpace::DeviceCMYK;
                                            components = 4;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            b"IM" | b"ImageMask" => {
                                if val_obj.as_bool() == Some(true) {
                                    is_mask = true;
                                    bpc = 1;
                                    components = 1;
                                }
                            }
                            b"F" | b"Filter" => {
                                if let Some(name) = val_obj.as_name() {
                                    match name {
                                        b"DCT" | b"DCTDecode" => encoding = ImageEncoding::Jpeg,
                                        b"Fl" | b"FlateDecode" => encoding = ImageEncoding::Flate,
                                        b"LZW" | b"LZWDecode" => encoding = ImageEncoding::Lzw,
                                        b"RL" | b"RunLengthDecode" => {
                                            encoding = ImageEncoding::RunLength
                                        }
                                        b"CCF" | b"CCITTFaxDecode" => {
                                            encoding = ImageEncoding::Ccitt
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        i += 1;
    }

    // After ID, the next token should be the image data
    if i < tokens.len() {
        if let CsToken::Operand(PdfObject::String(ref img_data)) = tokens[i] {
            let image_type = if is_mask {
                ImageType::Stencil
            } else {
                ImageType::Image
            };
            let img = PdfImage {
                index: *img_counter,
                page: page_num,
                obj_ref: None,
                width,
                height,
                bpc,
                color_space,
                components,
                encoding,
                image_type,
                interpolate: false,
                data: ImageData::Pixels(img_data.clone()),
            };
            *img_counter += 1;
            return Some(img);
        }
    }
    None
}

/// XObject entry - either an Image or a Form XObject.
enum XObjectEntry {
    Image(PdfObject, Option<Ref>),
    Form(PdfObject),
}

/// Build a map from XObject name -> XObjectEntry for all XObjects in a resource dict.
fn build_full_xobject_map<'a>(
    doc: &'a Document<'a>,
    resources: Option<&PdfObject>,
) -> std::collections::HashMap<Vec<u8>, XObjectEntry> {
    let mut map = std::collections::HashMap::new();

    let resources = match resources {
        Some(r) => match doc.resolve_obj(r) {
            Ok(resolved) => resolved,
            Err(_) => return map,
        },
        None => return map,
    };

    let xobject_dict = match resources.dict_get(b"XObject") {
        Some(xo) => match doc.resolve_obj(xo) {
            Ok(resolved) => resolved,
            Err(_) => return map,
        },
        None => return map,
    };

    if let Some(entries) = xobject_dict.as_dict() {
        for (name, val) in entries {
            let obj_ref = val.as_ref();
            if let Ok(resolved) = doc.resolve_obj(val) {
                let subtype = resolved
                    .dict_get(b"Subtype")
                    .and_then(|s| s.as_name_str())
                    .unwrap_or("");
                match subtype {
                    "Image" => {
                        map.insert(name.clone(), XObjectEntry::Image(resolved, obj_ref));
                    }
                    "Form" => {
                        map.insert(name.clone(), XObjectEntry::Form(resolved));
                    }
                    _ => {}
                }
            }
        }
    }

    map
}

/// IE12: List image metadata from a single page without extracting data.
pub fn list_images(
    doc: &Document,
    page: &Page,
    page_num: u32,
    img_counter: &mut u32,
    seen_refs: &mut HashSet<(u32, u16)>,
) -> Result<Vec<PdfImage>> {
    let mut results = Vec::new();

    let xobjects = find_image_xobjects(doc, page.resources.as_ref());

    for (_name, img_obj, obj_ref) in &xobjects {
        // IE13: Deduplication
        if let Some(r) = obj_ref {
            let key = (r.num, r.generation);
            if seen_refs.contains(&key) {
                continue;
            }
            seen_refs.insert(key);
        }

        let (width, height, bpc, color_space, components, encoding, image_type, interpolate) =
            parse_image_properties(&img_obj);

        let ref_tuple = obj_ref.map(|r| (r.num, r.generation));

        results.push(PdfImage {
            index: *img_counter,
            page: page_num,
            obj_ref: ref_tuple,
            width,
            height,
            bpc,
            color_space,
            components,
            encoding,
            image_type,
            interpolate,
            data: ImageData::Empty,
        });
        *img_counter += 1;
    }

    Ok(results)
}

/// Extract all images from the entire document.
///
/// Deduplicates across pages: if the same image object is referenced
/// from multiple pages, it is extracted only once.
pub fn extract_all_images(doc: &Document) -> Result<Vec<PdfImage>> {
    let pages = doc.pages()?;
    let mut results = Vec::new();
    let mut counter = 0u32;
    let mut seen = HashSet::new();

    for (i, page) in pages.iter().enumerate() {
        let page_images = extract_images(doc, page, i as u32, &mut counter, &mut seen)?;
        results.extend(page_images);
    }

    Ok(results)
}

/// List all images in the entire document (metadata only, no extraction).
pub fn list_all_images(doc: &Document) -> Result<Vec<PdfImage>> {
    let pages = doc.pages()?;
    let mut results = Vec::new();
    let mut counter = 0u32;
    let mut seen = HashSet::new();

    for (i, page) in pages.iter().enumerate() {
        let page_images = list_images(doc, page, i as u32, &mut counter, &mut seen)?;
        results.extend(page_images);
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// IE8: Inline image support
// ---------------------------------------------------------------------------

/// IE8: Extract an inline image from parsed BI/ID/EI data.
///
/// Inline images are embedded directly in the content stream.
/// They cannot be passthrough-extracted - always decoded to pixels.
pub fn extract_inline_image(
    dict: &PdfObject,
    data: &[u8],
    page_num: u32,
    img_index: u32,
) -> Result<PdfImage> {
    let (width, height, bpc, color_space, components, encoding, image_type, interpolate) =
        parse_image_properties(dict);

    // Inline images: apply all filters (no passthrough)
    let decoded = decode::decode_stream(dict, data)?;

    Ok(PdfImage {
        index: img_index,
        page: page_num,
        obj_ref: None, // Inline images have no object reference
        width,
        height,
        bpc,
        color_space,
        components,
        encoding,
        image_type,
        interpolate,
        data: ImageData::Pixels(decoded),
    })
}

// ---------------------------------------------------------------------------
// Convenience: file extension for extracted images
// ---------------------------------------------------------------------------

impl PdfImage {
    /// Suggested file extension for the extracted image.
    pub fn extension(&self) -> &'static str {
        match &self.data {
            ImageData::Passthrough(_) => match self.encoding {
                ImageEncoding::Jpeg => "jpg",
                ImageEncoding::Jpeg2000 => "jp2",
                _ => "bin",
            },
            ImageData::Jbig2 { .. } => "jb2e",
            ImageData::Ccitt { .. } => "ccitt",
            ImageData::Pixels(_) => {
                if self.image_type == ImageType::Stencil || self.bpc == 1 {
                    "pbm"
                } else {
                    "ppm"
                }
            }
            ImageData::Empty => "bin",
        }
    }

    /// Size of the extracted data in bytes.
    pub fn data_size(&self) -> usize {
        match &self.data {
            ImageData::Passthrough(d) => d.len(),
            ImageData::Jbig2 { page_data, globals } => {
                page_data.len() + globals.as_ref().map(|g| g.len()).unwrap_or(0)
            }
            ImageData::Ccitt { data, .. } => data.len(),
            ImageData::Pixels(d) => d.len(),
            ImageData::Empty => 0,
        }
    }

    /// Human-readable encoding name.
    pub fn encoding_name(&self) -> &'static str {
        match self.encoding {
            ImageEncoding::Jpeg => "jpeg",
            ImageEncoding::Jpeg2000 => "jpx",
            ImageEncoding::Jbig2 => "jbig2",
            ImageEncoding::Ccitt => "ccitt",
            ImageEncoding::Flate => "image",
            ImageEncoding::Lzw => "image",
            ImageEncoding::RunLength => "image",
            ImageEncoding::Raw => "image",
        }
    }

    /// Human-readable color space name.
    pub fn color_space_name(&self) -> &'static str {
        match &self.color_space {
            ImageColorSpace::DeviceGray => "gray",
            ImageColorSpace::DeviceRGB => "rgb",
            ImageColorSpace::DeviceCMYK => "cmyk",
            ImageColorSpace::CalGray => "gray",
            ImageColorSpace::CalRGB => "rgb",
            ImageColorSpace::ICCBased { .. } => "icc",
            ImageColorSpace::Indexed { .. } => "index",
            ImageColorSpace::Separation => "sep",
            ImageColorSpace::DeviceN => "devn",
            ImageColorSpace::Unknown => "-",
        }
    }

    /// Human-readable image type name.
    pub fn type_name(&self) -> &'static str {
        match self.image_type {
            ImageType::Image => "image",
            ImageType::Stencil => "stencil",
            ImageType::SoftMask => "smask",
            ImageType::Mask => "mask",
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- IE1: Find image XObjects ---

    #[test]
    fn ie1_no_images() {
        let doc = make_test_doc();
        let page = doc.pages().unwrap();
        let page = &page[0];

        let xobjects = find_image_xobjects(&doc, page.resources.as_ref());
        assert!(xobjects.is_empty());
    }

    // --- IE2: Image properties ---

    #[test]
    fn ie2_parse_rgb_jpeg() {
        let img_dict = PdfObject::Dict(vec![
            (b"Type".to_vec(), PdfObject::Name(b"XObject".to_vec())),
            (b"Subtype".to_vec(), PdfObject::Name(b"Image".to_vec())),
            (b"Width".to_vec(), PdfObject::Int(640)),
            (b"Height".to_vec(), PdfObject::Int(480)),
            (b"BitsPerComponent".to_vec(), PdfObject::Int(8)),
            (
                b"ColorSpace".to_vec(),
                PdfObject::Name(b"DeviceRGB".to_vec()),
            ),
            (b"Filter".to_vec(), PdfObject::Name(b"DCTDecode".to_vec())),
        ]);

        let (w, h, bpc, cs, comp, enc, itype, interp) = parse_image_properties(&img_dict);
        assert_eq!(w, 640);
        assert_eq!(h, 480);
        assert_eq!(bpc, 8);
        assert_eq!(cs, ImageColorSpace::DeviceRGB);
        assert_eq!(comp, 3);
        assert_eq!(enc, ImageEncoding::Jpeg);
        assert_eq!(itype, ImageType::Image);
        assert!(!interp);
    }

    #[test]
    fn ie2_parse_gray_flate() {
        let img_dict = PdfObject::Dict(vec![
            (b"Width".to_vec(), PdfObject::Int(100)),
            (b"Height".to_vec(), PdfObject::Int(100)),
            (b"BitsPerComponent".to_vec(), PdfObject::Int(8)),
            (
                b"ColorSpace".to_vec(),
                PdfObject::Name(b"DeviceGray".to_vec()),
            ),
            (b"Filter".to_vec(), PdfObject::Name(b"FlateDecode".to_vec())),
        ]);

        let (w, h, bpc, cs, comp, enc, _, _) = parse_image_properties(&img_dict);
        assert_eq!(w, 100);
        assert_eq!(h, 100);
        assert_eq!(bpc, 8);
        assert_eq!(cs, ImageColorSpace::DeviceGray);
        assert_eq!(comp, 1);
        assert_eq!(enc, ImageEncoding::Flate);
    }

    #[test]
    fn ie2_parse_cmyk() {
        let img_dict = PdfObject::Dict(vec![
            (b"Width".to_vec(), PdfObject::Int(200)),
            (b"Height".to_vec(), PdfObject::Int(300)),
            (b"BitsPerComponent".to_vec(), PdfObject::Int(8)),
            (
                b"ColorSpace".to_vec(),
                PdfObject::Name(b"DeviceCMYK".to_vec()),
            ),
            (b"Filter".to_vec(), PdfObject::Name(b"DCTDecode".to_vec())),
        ]);

        let (_, _, _, cs, comp, _, _, _) = parse_image_properties(&img_dict);
        assert_eq!(cs, ImageColorSpace::DeviceCMYK);
        assert_eq!(comp, 4);
    }

    // --- IE9: Stencil mask ---

    #[test]
    fn ie9_stencil_mask() {
        let img_dict = PdfObject::Dict(vec![
            (b"Width".to_vec(), PdfObject::Int(50)),
            (b"Height".to_vec(), PdfObject::Int(50)),
            (b"ImageMask".to_vec(), PdfObject::Bool(true)),
        ]);

        let (w, h, bpc, cs, comp, enc, itype, _) = parse_image_properties(&img_dict);
        assert_eq!(w, 50);
        assert_eq!(h, 50);
        assert_eq!(bpc, 1);
        assert_eq!(cs, ImageColorSpace::DeviceGray);
        assert_eq!(comp, 1);
        assert_eq!(enc, ImageEncoding::Raw);
        assert_eq!(itype, ImageType::Stencil);
    }

    // --- IE3: JPEG passthrough ---

    #[test]
    fn ie3_jpeg_passthrough() {
        // Minimal JPEG: SOI + EOI
        let jpeg_data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x02, 0xFF, 0xD9];

        let img_obj = PdfObject::Stream {
            dict: vec![
                (b"Width".to_vec(), PdfObject::Int(1)),
                (b"Height".to_vec(), PdfObject::Int(1)),
                (b"BitsPerComponent".to_vec(), PdfObject::Int(8)),
                (
                    b"ColorSpace".to_vec(),
                    PdfObject::Name(b"DeviceRGB".to_vec()),
                ),
                (b"Filter".to_vec(), PdfObject::Name(b"DCTDecode".to_vec())),
            ],
            data: jpeg_data.clone(),
        };

        let doc = make_test_doc();
        let result = extract_image_data(&doc, &img_obj, ImageEncoding::Jpeg, 1).unwrap();

        match result {
            ImageData::Passthrough(data) => {
                assert_eq!(data, jpeg_data, "JPEG data should be bit-identical");
            }
            _ => panic!("Expected Passthrough, got {:?}", result),
        }
    }

    // --- IE5: JPEG2000 passthrough ---

    #[test]
    fn ie5_jp2_passthrough() {
        // Minimal JP2 signature
        let jp2_data = vec![0x00, 0x00, 0x00, 0x0C, 0x6A, 0x50, 0x20, 0x20];

        let img_obj = PdfObject::Stream {
            dict: vec![
                (b"Width".to_vec(), PdfObject::Int(1)),
                (b"Height".to_vec(), PdfObject::Int(1)),
                (b"Filter".to_vec(), PdfObject::Name(b"JPXDecode".to_vec())),
            ],
            data: jp2_data.clone(),
        };

        let doc = make_test_doc();
        let result = extract_image_data(&doc, &img_obj, ImageEncoding::Jpeg2000, 1).unwrap();

        match result {
            ImageData::Passthrough(data) => {
                assert_eq!(data, jp2_data, "JP2 data should be bit-identical");
            }
            _ => panic!("Expected Passthrough"),
        }
    }

    // --- IE6: CCITT passthrough + params ---

    #[test]
    fn ie6_ccitt_passthrough() {
        let ccitt_data = vec![0x00, 0x01, 0x02, 0x03];

        let img_obj = PdfObject::Stream {
            dict: vec![
                (b"Width".to_vec(), PdfObject::Int(1728)),
                (b"Height".to_vec(), PdfObject::Int(2200)),
                (b"BitsPerComponent".to_vec(), PdfObject::Int(1)),
                (
                    b"ColorSpace".to_vec(),
                    PdfObject::Name(b"DeviceGray".to_vec()),
                ),
                (
                    b"Filter".to_vec(),
                    PdfObject::Name(b"CCITTFaxDecode".to_vec()),
                ),
                (
                    b"DecodeParms".to_vec(),
                    PdfObject::Dict(vec![
                        (b"K".to_vec(), PdfObject::Int(-1)),
                        (b"Columns".to_vec(), PdfObject::Int(1728)),
                        (b"Rows".to_vec(), PdfObject::Int(2200)),
                        (b"EndOfLine".to_vec(), PdfObject::Bool(false)),
                        (b"BlackIs1".to_vec(), PdfObject::Bool(true)),
                    ]),
                ),
            ],
            data: ccitt_data.clone(),
        };

        let doc = make_test_doc();
        let result = extract_image_data(&doc, &img_obj, ImageEncoding::Ccitt, 2200).unwrap();

        match result {
            ImageData::Ccitt { data, params } => {
                assert_eq!(data, ccitt_data, "CCITT data should be bit-identical");
                assert_eq!(params.encoding, CcittEncoding::Group4);
                assert_eq!(params.columns, 1728);
                assert_eq!(params.rows, 2200);
                assert!(!params.end_of_line);
                assert!(params.black_is_1);
            }
            _ => panic!("Expected Ccitt"),
        }
    }

    #[test]
    fn ie6_ccitt_group3_1d() {
        let img_obj = PdfObject::Stream {
            dict: vec![
                (b"Width".to_vec(), PdfObject::Int(200)),
                (b"Height".to_vec(), PdfObject::Int(100)),
                (
                    b"Filter".to_vec(),
                    PdfObject::Name(b"CCITTFaxDecode".to_vec()),
                ),
                (
                    b"DecodeParms".to_vec(),
                    PdfObject::Dict(vec![
                        (b"K".to_vec(), PdfObject::Int(0)),
                        (b"Columns".to_vec(), PdfObject::Int(200)),
                        (b"EndOfLine".to_vec(), PdfObject::Bool(true)),
                    ]),
                ),
            ],
            data: vec![0x00],
        };

        let doc = make_test_doc();
        let result = extract_image_data(&doc, &img_obj, ImageEncoding::Ccitt, 100).unwrap();

        if let ImageData::Ccitt { params, .. } = result {
            assert_eq!(params.encoding, CcittEncoding::Group3_1D);
            assert_eq!(params.columns, 200);
            assert!(params.end_of_line);
            assert!(!params.black_is_1);
        } else {
            panic!("Expected Ccitt");
        }
    }

    #[test]
    fn ie6_ccitt_group3_2d() {
        let img_obj = PdfObject::Stream {
            dict: vec![
                (b"Width".to_vec(), PdfObject::Int(100)),
                (b"Height".to_vec(), PdfObject::Int(50)),
                (
                    b"Filter".to_vec(),
                    PdfObject::Name(b"CCITTFaxDecode".to_vec()),
                ),
                (
                    b"DecodeParms".to_vec(),
                    PdfObject::Dict(vec![(b"K".to_vec(), PdfObject::Int(4))]),
                ),
            ],
            data: vec![0x00],
        };

        let doc = make_test_doc();
        let result = extract_image_data(&doc, &img_obj, ImageEncoding::Ccitt, 50).unwrap();

        if let ImageData::Ccitt { params, .. } = result {
            assert_eq!(params.encoding, CcittEncoding::Group3_2D);
        } else {
            panic!("Expected Ccitt");
        }
    }

    #[test]
    fn ie6_ccitt_defaults() {
        // No DecodeParms at all - should use defaults
        let img_obj = PdfObject::Stream {
            dict: vec![
                (b"Width".to_vec(), PdfObject::Int(100)),
                (b"Height".to_vec(), PdfObject::Int(50)),
                (
                    b"Filter".to_vec(),
                    PdfObject::Name(b"CCITTFaxDecode".to_vec()),
                ),
            ],
            data: vec![0x00],
        };

        let doc = make_test_doc();
        let result = extract_image_data(&doc, &img_obj, ImageEncoding::Ccitt, 50).unwrap();

        if let ImageData::Ccitt { params, .. } = result {
            assert_eq!(params.encoding, CcittEncoding::Group3_1D); // K=0 default
            assert_eq!(params.columns, 1728); // default
            assert_eq!(params.rows, 50); // falls back to height
            assert!(!params.end_of_line);
            assert!(!params.black_is_1);
        } else {
            panic!("Expected Ccitt");
        }
    }

    // --- IE7: JBIG2 ---

    #[test]
    fn ie7_jbig2_no_globals() {
        let jbig2_data = vec![0x97, 0x4A, 0x42, 0x32]; // arbitrary

        let img_obj = PdfObject::Stream {
            dict: vec![
                (b"Width".to_vec(), PdfObject::Int(100)),
                (b"Height".to_vec(), PdfObject::Int(100)),
                (b"Filter".to_vec(), PdfObject::Name(b"JBIG2Decode".to_vec())),
            ],
            data: jbig2_data.clone(),
        };

        let doc = make_test_doc();
        let result = extract_image_data(&doc, &img_obj, ImageEncoding::Jbig2, 100).unwrap();

        match result {
            ImageData::Jbig2 { page_data, globals } => {
                assert_eq!(page_data, jbig2_data);
                assert!(globals.is_none());
            }
            _ => panic!("Expected Jbig2"),
        }
    }

    // --- IE4: Flate -> pixels ---

    #[test]
    fn ie4_flate_decode() {
        // Create valid flate-compressed data: 3 bytes (1 RGB pixel)
        let raw_pixels = vec![0xFF, 0x00, 0x80]; // one red-ish pixel
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw_pixels, 6);

        let img_obj = PdfObject::Stream {
            dict: vec![
                (b"Width".to_vec(), PdfObject::Int(1)),
                (b"Height".to_vec(), PdfObject::Int(1)),
                (b"BitsPerComponent".to_vec(), PdfObject::Int(8)),
                (
                    b"ColorSpace".to_vec(),
                    PdfObject::Name(b"DeviceRGB".to_vec()),
                ),
                (b"Filter".to_vec(), PdfObject::Name(b"FlateDecode".to_vec())),
            ],
            data: compressed,
        };

        let doc = make_test_doc();
        let result = extract_image_data(&doc, &img_obj, ImageEncoding::Flate, 1).unwrap();

        match result {
            ImageData::Pixels(pixels) => {
                assert_eq!(pixels, raw_pixels);
            }
            _ => panic!("Expected Pixels"),
        }
    }

    // --- IE11: Color space parsing ---

    #[test]
    fn ie11_indexed_color_space() {
        let img_dict = PdfObject::Dict(vec![
            (b"Width".to_vec(), PdfObject::Int(10)),
            (b"Height".to_vec(), PdfObject::Int(10)),
            (b"BitsPerComponent".to_vec(), PdfObject::Int(8)),
            (
                b"ColorSpace".to_vec(),
                PdfObject::Array(vec![
                    PdfObject::Name(b"Indexed".to_vec()),
                    PdfObject::Name(b"DeviceRGB".to_vec()),
                    PdfObject::Int(255),
                    PdfObject::String(vec![0; 768]), // 256 RGB entries
                ]),
            ),
        ]);

        let (_, _, _, cs, comp, _, _, _) = parse_image_properties(&img_dict);
        assert_eq!(comp, 1); // Indexed = 1 component (the index)
        match cs {
            ImageColorSpace::Indexed { base, num_colors } => {
                assert_eq!(*base, ImageColorSpace::DeviceRGB);
                assert_eq!(num_colors, 256);
            }
            _ => panic!("Expected Indexed color space"),
        }
    }

    #[test]
    fn ie11_icc_color_space() {
        let icc_stream = PdfObject::Dict(vec![(b"N".to_vec(), PdfObject::Int(4))]);

        let img_dict = PdfObject::Dict(vec![
            (b"Width".to_vec(), PdfObject::Int(10)),
            (b"Height".to_vec(), PdfObject::Int(10)),
            (b"BitsPerComponent".to_vec(), PdfObject::Int(8)),
            (
                b"ColorSpace".to_vec(),
                PdfObject::Array(vec![PdfObject::Name(b"ICCBased".to_vec()), icc_stream]),
            ),
        ]);

        let (_, _, _, cs, comp, _, _, _) = parse_image_properties(&img_dict);
        assert_eq!(comp, 4);
        match cs {
            ImageColorSpace::ICCBased { components } => assert_eq!(components, 4),
            _ => panic!("Expected ICCBased"),
        }
    }

    // --- IE12: Image info methods ---

    #[test]
    fn ie12_extension() {
        let jpeg_img = PdfImage {
            index: 0,
            page: 0,
            obj_ref: None,
            width: 100,
            height: 100,
            bpc: 8,
            color_space: ImageColorSpace::DeviceRGB,
            components: 3,
            encoding: ImageEncoding::Jpeg,
            image_type: ImageType::Image,
            interpolate: false,
            data: ImageData::Passthrough(vec![]),
        };
        assert_eq!(jpeg_img.extension(), "jpg");

        let ccitt_img = PdfImage {
            index: 1,
            page: 0,
            obj_ref: None,
            width: 100,
            height: 100,
            bpc: 1,
            color_space: ImageColorSpace::DeviceGray,
            components: 1,
            encoding: ImageEncoding::Ccitt,
            image_type: ImageType::Image,
            interpolate: false,
            data: ImageData::Ccitt {
                data: vec![],
                params: CcittParams {
                    encoding: CcittEncoding::Group4,
                    columns: 100,
                    rows: 100,
                    end_of_line: false,
                    black_is_1: false,
                    encoded_byte_align: false,
                },
            },
        };
        assert_eq!(ccitt_img.extension(), "ccitt");

        let pixel_img = PdfImage {
            index: 2,
            page: 0,
            obj_ref: None,
            width: 10,
            height: 10,
            bpc: 8,
            color_space: ImageColorSpace::DeviceRGB,
            components: 3,
            encoding: ImageEncoding::Flate,
            image_type: ImageType::Image,
            interpolate: false,
            data: ImageData::Pixels(vec![]),
        };
        assert_eq!(pixel_img.extension(), "ppm");

        let stencil = PdfImage {
            index: 3,
            page: 0,
            obj_ref: None,
            width: 10,
            height: 10,
            bpc: 1,
            color_space: ImageColorSpace::DeviceGray,
            components: 1,
            encoding: ImageEncoding::Raw,
            image_type: ImageType::Stencil,
            interpolate: false,
            data: ImageData::Pixels(vec![]),
        };
        assert_eq!(stencil.extension(), "pbm");
    }

    #[test]
    fn ie12_encoding_name() {
        assert_eq!(
            PdfImage {
                index: 0,
                page: 0,
                obj_ref: None,
                width: 1,
                height: 1,
                bpc: 8,
                color_space: ImageColorSpace::DeviceRGB,
                components: 3,
                encoding: ImageEncoding::Jpeg,
                image_type: ImageType::Image,
                interpolate: false,
                data: ImageData::Empty,
            }
            .encoding_name(),
            "jpeg"
        );
    }

    #[test]
    fn ie12_type_name() {
        assert_eq!(
            PdfImage {
                index: 0,
                page: 0,
                obj_ref: None,
                width: 1,
                height: 1,
                bpc: 1,
                color_space: ImageColorSpace::DeviceGray,
                components: 1,
                encoding: ImageEncoding::Raw,
                image_type: ImageType::Stencil,
                interpolate: false,
                data: ImageData::Empty,
            }
            .type_name(),
            "stencil"
        );
    }

    // --- IE13: Deduplication ---

    #[test]
    fn ie13_dedup_tracking() {
        let mut seen = HashSet::new();
        let r1 = Ref {
            num: 5,
            generation: 0,
        };

        let key = (r1.num, r1.generation);
        assert!(!seen.contains(&key));
        seen.insert(key);
        assert!(seen.contains(&key));

        // Same ref should be skipped
        let r2 = Ref {
            num: 5,
            generation: 0,
        };
        let key2 = (r2.num, r2.generation);
        assert!(seen.contains(&key2));
    }

    // --- Encoding detection ---

    #[test]
    fn encoding_detection() {
        let jpeg = PdfObject::Dict(vec![(
            b"Filter".to_vec(),
            PdfObject::Name(b"DCTDecode".to_vec()),
        )]);
        assert_eq!(parse_encoding(&jpeg), ImageEncoding::Jpeg);

        let jp2 = PdfObject::Dict(vec![(
            b"Filter".to_vec(),
            PdfObject::Name(b"JPXDecode".to_vec()),
        )]);
        assert_eq!(parse_encoding(&jp2), ImageEncoding::Jpeg2000);

        let jbig2 = PdfObject::Dict(vec![(
            b"Filter".to_vec(),
            PdfObject::Name(b"JBIG2Decode".to_vec()),
        )]);
        assert_eq!(parse_encoding(&jbig2), ImageEncoding::Jbig2);

        let ccitt = PdfObject::Dict(vec![(
            b"Filter".to_vec(),
            PdfObject::Name(b"CCITTFaxDecode".to_vec()),
        )]);
        assert_eq!(parse_encoding(&ccitt), ImageEncoding::Ccitt);

        let flate = PdfObject::Dict(vec![(
            b"Filter".to_vec(),
            PdfObject::Name(b"FlateDecode".to_vec()),
        )]);
        assert_eq!(parse_encoding(&flate), ImageEncoding::Flate);

        let none = PdfObject::Dict(vec![]);
        assert_eq!(parse_encoding(&none), ImageEncoding::Raw);
    }

    // --- Multi-filter chain ---

    #[test]
    fn multi_filter_chain_detection() {
        // [/ASCII85Decode /DCTDecode] - last filter is image-format
        let img_dict = PdfObject::Dict(vec![(
            b"Filter".to_vec(),
            PdfObject::Array(vec![
                PdfObject::Name(b"ASCII85Decode".to_vec()),
                PdfObject::Name(b"DCTDecode".to_vec()),
            ]),
        )]);

        let enc = parse_encoding(&img_dict);
        assert_eq!(enc, ImageEncoding::Jpeg);

        let filters = decode::get_filters(&img_dict);
        assert!(is_passthrough_filter(filters.last().unwrap()));
    }

    // --- Passthrough verification ---

    #[test]
    fn passthrough_is_exact() {
        // The defining property: extracted bytes == embedded bytes
        let original = vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, // JPEG SOI + APP0
            0x4A, 0x46, 0x49, 0x46, 0x00, // "JFIF\0"
            0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xD9, // EOI
        ];

        let img_obj = PdfObject::Stream {
            dict: vec![
                (b"Filter".to_vec(), PdfObject::Name(b"DCTDecode".to_vec())),
                (b"Width".to_vec(), PdfObject::Int(1)),
                (b"Height".to_vec(), PdfObject::Int(1)),
            ],
            data: original.clone(),
        };

        let doc = make_test_doc();
        let result = extract_image_data(&doc, &img_obj, ImageEncoding::Jpeg, 1).unwrap();

        if let ImageData::Passthrough(extracted) = result {
            assert_eq!(extracted.len(), original.len());
            assert_eq!(extracted, original, "Passthrough must be bit-identical");
        } else {
            panic!("Expected Passthrough");
        }
    }

    // --- Helper ---

    fn make_test_doc() -> Document<'static> {
        static PDF: &[u8] = b"%PDF-1.7\n\
            1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
            2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
            3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>\nendobj\n\
            xref\n0 4\n\
            0000000000 65535 f \n\
            0000000009 00000 n \n\
            0000000058 00000 n \n\
            0000000115 00000 n \n\
            trailer\n<< /Size 4 /Root 1 0 R >>\n\
            startxref\n191\n%%EOF";
        Document::parse(PDF).unwrap()
    }
}
