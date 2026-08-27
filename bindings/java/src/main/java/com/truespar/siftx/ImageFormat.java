package com.truespar.siftx;

/** Format of an extracted PDF image. */
public enum ImageFormat {
    JPEG(0, "jpg"),
    JPEG2000(1, "jp2"),
    JBIG2(2, "jb2"),
    CCITT(3, "tiff"),
    PIXELS(4, "ppm");

    private final int code;
    private final String extension;

    ImageFormat(int code, String extension) {
        this.code = code;
        this.extension = extension;
    }

    public int code() { return code; }
    public String extension() { return extension; }

    static ImageFormat fromCode(int code) {
        for (var f : values()) {
            if (f.code == code) return f;
        }
        return PIXELS;
    }
}
