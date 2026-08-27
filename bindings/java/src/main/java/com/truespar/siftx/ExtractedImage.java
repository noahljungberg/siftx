package com.truespar.siftx;

/**
 * An image extracted from a PDF document.
 *
 * @param page           0-based page index.
 * @param width          Pixel width.
 * @param height         Pixel height.
 * @param bitsPerComponent Bits per component (1, 2, 4, 8, 16).
 * @param components     Number of color components (1=gray, 3=RGB, 4=CMYK).
 * @param format         Image data format.
 * @param data           Raw image data bytes.
 */
public record ExtractedImage(
        int page,
        int width,
        int height,
        int bitsPerComponent,
        int components,
        ImageFormat format,
        byte[] data
) {
    /** Suggested file extension (e.g., "jpg", "jp2", "ppm"). */
    public String extension() {
        return format.extension();
    }

    /** Whether the image data can be written directly to disk (JPEG/JP2K passthrough). */
    public boolean isPassthrough() {
        return format == ImageFormat.JPEG || format == ImageFormat.JPEG2000;
    }
}
