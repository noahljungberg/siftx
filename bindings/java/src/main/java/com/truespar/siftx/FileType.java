package com.truespar.siftx;

/** Detected file type. */
public enum FileType {
    UNKNOWN(0),
    JPEG(1),
    PNG(2),
    GIF(3),
    BMP(4),
    TIFF(5),
    WEBP(6),
    HEIF(7),
    PDF(8),
    ICC(9),
    QUICKTIME(10);

    private final int code;

    FileType(int code) { this.code = code; }

    public int code() { return code; }

    static FileType fromCode(int code) {
        for (var ft : values()) {
            if (ft.code == code) return ft;
        }
        return UNKNOWN;
    }
}
