package com.truespar.siftx;

import java.io.IOException;
import java.io.InputStream;
import java.lang.foreign.*;
import java.lang.invoke.MethodHandle;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.Locale;

/**
 * Raw Panama FFM bindings to the Sift native library.
 * All method handles are resolved lazily on first use.
 */
final class Native {
    private Native() {}

    // -----------------------------------------------------------------------
    // Library loading
    // -----------------------------------------------------------------------

    private static final SymbolLookup LIB;

    static {
        loadNativeLibrary();
        LIB = SymbolLookup.loaderLookup();
    }

    /**
     * Locate and load libsiftx, in the order a caller would expect.
     *
     * <ol>
     *   <li>An explicit path, from the {@code siftx.native.lib} system property
     *       or the {@code SIFTX_NATIVE_LIB_PATH} environment variable. This is
     *       what a developer building the Rust side locally wants.</li>
     *   <li>The copy bundled in this JAR, extracted to a temporary file. This
     *       is the path an ordinary Maven consumer takes, and the reason they
     *       do not have to install anything.</li>
     *   <li>The system library path, for a platform we do not ship a binary
     *       for but where the user has installed one.</li>
     * </ol>
     */
    private static void loadNativeLibrary() {
        // Blank counts as absent. An empty SIFTX_NATIVE_LIB_PATH is easy to
        // produce by accident - an unset shell variable expanded into a
        // command, `ENV SIFTX_NATIVE_LIB_PATH=` in a Dockerfile - and
        // System.load throws on it rather than falling through, which would
        // turn a harmless empty value into a total failure to load.
        var explicit = blankToNull(System.getProperty("siftx.native.lib"));
        if (explicit == null) {
            explicit = blankToNull(System.getenv("SIFTX_NATIVE_LIB_PATH"));
        }
        if (explicit != null) {
            System.load(explicit);
            return;
        }

        var resource = bundledResourcePath();
        if (resource != null) {
            try (var in = Native.class.getResourceAsStream(resource)) {
                if (in != null) {
                    System.load(extractToTempFile(in, resource).toString());
                    return;
                }
            } catch (IOException e) {
                throw new UnsatisfiedLinkError(
                        "failed to extract bundled native library " + resource + ": " + e);
            }
        }

        try {
            System.loadLibrary("siftx");
        } catch (UnsatisfiedLinkError e) {
            throw new UnsatisfiedLinkError(
                    "no native siftx library for " + System.getProperty("os.name") + "/"
                            + System.getProperty("os.arch")
                            + ". This JAR bundles binaries for Linux, macOS and Windows on x86-64"
                            + " and arm64; for anything else, build the Rust library and point"
                            + " SIFTX_NATIVE_LIB_PATH at it. Original error: " + e.getMessage());
        }
    }

    private static String blankToNull(String s) {
        return (s == null || s.isBlank()) ? null : s;
    }

    /**
     * Resource path of the bundled library for this platform, or null if this
     * platform is not one we ship.
     */
    private static String bundledResourcePath() {
        var os = System.getProperty("os.name", "").toLowerCase(Locale.ROOT);
        var arch = System.getProperty("os.arch", "").toLowerCase(Locale.ROOT);

        String osDir;
        String file;
        if (os.contains("linux")) {
            osDir = "linux";
            file = "libsiftx.so";
        } else if (os.contains("mac") || os.contains("darwin")) {
            osDir = "darwin";
            file = "libsiftx.dylib";
        } else if (os.contains("windows")) {
            osDir = "windows";
            file = "siftx.dll";
        } else {
            return null;
        }

        // os.arch reports amd64 on most JVMs and x86_64 on some; likewise
        // aarch64 and arm64. Normalise both spellings.
        String archDir;
        if (arch.equals("amd64") || arch.equals("x86_64")) {
            archDir = "x86_64";
        } else if (arch.equals("aarch64") || arch.equals("arm64")) {
            archDir = "aarch64";
        } else {
            return null;
        }

        return "/native/" + osDir + "-" + archDir + "/" + file;
    }

    /**
     * Copy a bundled library out of the JAR so the OS loader can open it.
     * A library inside a JAR is not a file, and dlopen needs one.
     */
    private static Path extractToTempFile(InputStream in, String resource) throws IOException {
        var name = resource.substring(resource.lastIndexOf('/') + 1);
        var dot = name.lastIndexOf('.');
        var dir = Files.createTempDirectory("siftx-native");
        var out = dir.resolve(name);
        Files.copy(in, out, StandardCopyOption.REPLACE_EXISTING);

        // Delete on exit rather than immediately: the library stays mapped for
        // the life of the process, and Windows will not unlink an open DLL.
        out.toFile().deleteOnExit();
        dir.toFile().deleteOnExit();
        if (dot < 0) {
            throw new IOException("bundled library has no extension: " + name);
        }
        return out;
    }

    private static MethodHandle downcall(String name, FunctionDescriptor desc) {
        var addr = LIB.find(name)
                .orElseThrow(() -> new UnsatisfiedLinkError("Symbol not found: " + name));
        return Linker.nativeLinker().downcallHandle(addr, desc);
    }

    // -----------------------------------------------------------------------
    // Result codes (matches SiftXResult enum)
    // -----------------------------------------------------------------------

    static final int OK = 0;
    static final int INVALID_ARG = 1;
    static final int IO_ERROR = 2;
    static final int FORMAT_ERROR = 3;
    static final int TRUNCATED = 4;
    static final int UNSUPPORTED = 5;
    static final int INTERNAL_ERROR = 6;

    // -----------------------------------------------------------------------
    // File type constants (matches SiftXFileType enum)
    // -----------------------------------------------------------------------

    static final int TYPE_UNKNOWN = 0;
    static final int TYPE_JPEG = 1;
    static final int TYPE_PNG = 2;
    static final int TYPE_GIF = 3;
    static final int TYPE_BMP = 4;
    static final int TYPE_TIFF = 5;
    static final int TYPE_WEBP = 6;
    static final int TYPE_HEIF = 7;
    static final int TYPE_PDF = 8;
    static final int TYPE_ICC = 9;
    static final int TYPE_QUICKTIME = 10;

    // -----------------------------------------------------------------------
    // Struct layouts
    // -----------------------------------------------------------------------

    // Tag struct: 3 pointers + typed value fields. The typed fields are not
    // surfaced on the Tag record yet, but they MUST be described here: the
    // layout sizes the buffer siftx_tags_get() writes into, and a short buffer
    // is a heap overflow, not a truncated read.
    static final StructLayout TAG_LAYOUT = MemoryLayout.structLayout(
            ValueLayout.ADDRESS.withName("group"),        // 0-7
            ValueLayout.ADDRESS.withName("name"),         // 8-15
            ValueLayout.ADDRESS.withName("value"),        // 16-23
            ValueLayout.JAVA_BYTE.withName("valueType"),  // 24
            MemoryLayout.paddingLayout(3),                // 25-27 (_pad[3])
            MemoryLayout.paddingLayout(4),                // 28-31 (align to 8)
            ValueLayout.JAVA_LONG.withName("intVal"),     // 32-39
            ValueLayout.JAVA_INT.withName("rationalNum"), // 40-43
            ValueLayout.JAVA_INT.withName("rationalDen"), // 44-47
            ValueLayout.JAVA_DOUBLE.withName("floatVal")  // 48-55
    );

    static final StructLayout GPS_LAYOUT = MemoryLayout.structLayout(
            ValueLayout.JAVA_DOUBLE.withName("latitude"),
            ValueLayout.JAVA_DOUBLE.withName("longitude"),
            ValueLayout.JAVA_DOUBLE.withName("altitude"),
            ValueLayout.JAVA_INT.withName("hasAltitude"),
            MemoryLayout.paddingLayout(4) // align to 8-byte boundary
    );

    static final StructLayout IMAGE_LAYOUT = MemoryLayout.structLayout(
            ValueLayout.JAVA_INT.withName("page"),      // 0-3
            ValueLayout.JAVA_INT.withName("width"),      // 4-7
            ValueLayout.JAVA_INT.withName("height"),     // 8-11
            ValueLayout.JAVA_BYTE.withName("bpc"),       // 12
            ValueLayout.JAVA_BYTE.withName("components"),// 13
            ValueLayout.JAVA_BYTE.withName("format"),    // 14
            MemoryLayout.paddingLayout(1),               // 15 (align to 16)
            ValueLayout.ADDRESS.withName("data"),        // 16-23
            ValueLayout.JAVA_LONG.withName("dataLen")    // 24-31
    );

    // Form field struct: 4 pointers + uint32 + int32 + int32
    static final StructLayout FORM_FIELD_LAYOUT = MemoryLayout.structLayout(
            ValueLayout.ADDRESS.withName("fieldType"),
            ValueLayout.ADDRESS.withName("name"),
            ValueLayout.ADDRESS.withName("value"),
            ValueLayout.ADDRESS.withName("defaultValue"),
            ValueLayout.JAVA_INT.withName("flags"),
            ValueLayout.JAVA_INT.withName("isReadOnly"),
            ValueLayout.JAVA_INT.withName("isRequired"),
            MemoryLayout.paddingLayout(4) // align to 8-byte boundary
    );

    // Annotation struct: annot_type(ptr), page(u32)+pad, rect[4](doubles), contents(ptr), dest(ptr), flags(u32), has_appearance(i32)
    static final StructLayout ANNOTATION_LAYOUT = MemoryLayout.structLayout(
            ValueLayout.ADDRESS.withName("annotType"),
            ValueLayout.JAVA_INT.withName("page"),
            MemoryLayout.paddingLayout(4),
            ValueLayout.JAVA_DOUBLE.withName("rectLlx"),
            ValueLayout.JAVA_DOUBLE.withName("rectLly"),
            ValueLayout.JAVA_DOUBLE.withName("rectUrx"),
            ValueLayout.JAVA_DOUBLE.withName("rectUry"),
            ValueLayout.ADDRESS.withName("contents"),
            ValueLayout.ADDRESS.withName("dest"),
            ValueLayout.JAVA_INT.withName("flags"),
            ValueLayout.JAVA_INT.withName("hasAppearance")
    );

    // Struct element: struct_type(ptr), depth(u32)+pad, title(ptr), alt_text(ptr), actual_text(ptr), lang(ptr)
    static final StructLayout STRUCT_ELEMENT_LAYOUT = MemoryLayout.structLayout(
            ValueLayout.ADDRESS.withName("structType"),
            ValueLayout.JAVA_INT.withName("depth"),
            MemoryLayout.paddingLayout(4),
            ValueLayout.ADDRESS.withName("title"),
            ValueLayout.ADDRESS.withName("altText"),
            ValueLayout.ADDRESS.withName("actualText"),
            ValueLayout.ADDRESS.withName("lang")
    );

    // -----------------------------------------------------------------------
    // Method handles
    // -----------------------------------------------------------------------

    // Error
    static final MethodHandle siftx_error_message = downcall("siftx_error_message",
            FunctionDescriptor.of(ValueLayout.ADDRESS));

    // Lifecycle
    static final MethodHandle siftx_open = downcall("siftx_open",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    static final MethodHandle siftx_file_type = downcall("siftx_file_type",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS));

    static final MethodHandle siftx_parse = downcall("siftx_parse",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    static final MethodHandle siftx_read = downcall("siftx_read",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.JAVA_LONG, ValueLayout.ADDRESS));

    static final MethodHandle siftx_file_free = downcall("siftx_file_free",
            FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));

    static final MethodHandle siftx_document_free = downcall("siftx_document_free",
            FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));

    // Tags
    static final MethodHandle siftx_tags = downcall("siftx_tags",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    static final MethodHandle siftx_tags_from_path = downcall("siftx_tags_from_path",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    static final MethodHandle siftx_tags_count = downcall("siftx_tags_count",
            FunctionDescriptor.of(ValueLayout.JAVA_LONG, ValueLayout.ADDRESS));

    static final MethodHandle siftx_tags_get = downcall("siftx_tags_get",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.JAVA_LONG, ValueLayout.ADDRESS));

    static final MethodHandle siftx_tags_free = downcall("siftx_tags_free",
            FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));

    // GPS
    static final MethodHandle siftx_gps = downcall("siftx_gps",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    // Thumbnail
    static final MethodHandle siftx_thumbnail = downcall("siftx_thumbnail",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    static final MethodHandle siftx_thumbnail_free = downcall("siftx_thumbnail_free",
            FunctionDescriptor.ofVoid(ValueLayout.ADDRESS, ValueLayout.JAVA_LONG));

    // Images
    static final MethodHandle siftx_images = downcall("siftx_images",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    static final MethodHandle siftx_images_count = downcall("siftx_images_count",
            FunctionDescriptor.of(ValueLayout.JAVA_LONG, ValueLayout.ADDRESS));

    static final MethodHandle siftx_images_get = downcall("siftx_images_get",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.JAVA_LONG, ValueLayout.ADDRESS));

    static final MethodHandle siftx_images_free = downcall("siftx_images_free",
            FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));

    // Text pages
    static final MethodHandle siftx_text_pages = downcall("siftx_text_pages",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    static final MethodHandle siftx_text_pages_count = downcall("siftx_text_pages_count",
            FunctionDescriptor.of(ValueLayout.JAVA_LONG, ValueLayout.ADDRESS));

    static final MethodHandle siftx_text_pages_get = downcall("siftx_text_pages_get",
            FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.JAVA_LONG));

    static final MethodHandle siftx_text_pages_free = downcall("siftx_text_pages_free",
            FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));

    // Filtered tags (same signature/iteration as siftx_tags)
    static final MethodHandle siftx_exif_tags = downcall("siftx_exif_tags",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    static final MethodHandle siftx_xmp_tags = downcall("siftx_xmp_tags",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    static final MethodHandle siftx_iptc_tags = downcall("siftx_iptc_tags",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    // Raw text pages (same signature/iteration as siftx_text_pages)
    static final MethodHandle siftx_text_pages_raw = downcall("siftx_text_pages_raw",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    // Authentication
    static final MethodHandle siftx_authenticate = downcall("siftx_authenticate",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.JAVA_LONG));

    // Form fields
    static final MethodHandle siftx_form_fields = downcall("siftx_form_fields",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    static final MethodHandle siftx_form_fields_count = downcall("siftx_form_fields_count",
            FunctionDescriptor.of(ValueLayout.JAVA_LONG, ValueLayout.ADDRESS));

    static final MethodHandle siftx_form_fields_get = downcall("siftx_form_fields_get",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.JAVA_LONG, ValueLayout.ADDRESS));

    static final MethodHandle siftx_form_fields_free = downcall("siftx_form_fields_free",
            FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));

    // Annotations
    static final MethodHandle siftx_annotations = downcall("siftx_annotations",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    static final MethodHandle siftx_annotations_count = downcall("siftx_annotations_count",
            FunctionDescriptor.of(ValueLayout.JAVA_LONG, ValueLayout.ADDRESS));

    static final MethodHandle siftx_annotations_get = downcall("siftx_annotations_get",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.JAVA_LONG, ValueLayout.ADDRESS));

    static final MethodHandle siftx_annotations_free = downcall("siftx_annotations_free",
            FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));

    // Structure tree
    static final MethodHandle siftx_struct_tree = downcall("siftx_struct_tree",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    static final MethodHandle siftx_struct_tree_count = downcall("siftx_struct_tree_count",
            FunctionDescriptor.of(ValueLayout.JAVA_LONG, ValueLayout.ADDRESS));

    static final MethodHandle siftx_struct_tree_get = downcall("siftx_struct_tree_get",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.JAVA_LONG, ValueLayout.ADDRESS));

    static final MethodHandle siftx_struct_tree_role_map_count = downcall("siftx_struct_tree_role_map_count",
            FunctionDescriptor.of(ValueLayout.JAVA_LONG, ValueLayout.ADDRESS));

    static final MethodHandle siftx_struct_tree_role_map_get = downcall("siftx_struct_tree_role_map_get",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS, ValueLayout.JAVA_LONG, ValueLayout.ADDRESS, ValueLayout.ADDRESS));

    static final MethodHandle siftx_struct_tree_free = downcall("siftx_struct_tree_free",
            FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));

    // Document info
    static final MethodHandle siftx_document_file_type = downcall("siftx_document_file_type",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.ADDRESS));

    // Version
    static final MethodHandle siftx_version = downcall("siftx_version",
            FunctionDescriptor.of(ValueLayout.ADDRESS));

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    static String readUtf8(MemorySegment ptr) {
        if (ptr.equals(MemorySegment.NULL)) return null;
        return ptr.reinterpret(Long.MAX_VALUE).getString(0);
    }

    static void throwOnError(int result) {
        if (result == OK) return;

        String msg;
        try {
            var ptr = (MemorySegment) siftx_error_message.invokeExact();
            msg = readUtf8(ptr);
        } catch (Throwable t) {
            msg = null;
        }
        if (msg == null) msg = "error code " + result;

        switch (result) {
            case INVALID_ARG -> throw new IllegalArgumentException(msg);
            case IO_ERROR -> throw new SiftIOException(msg);
            case FORMAT_ERROR, TRUNCATED -> throw new SiftFormatException(msg);
            case UNSUPPORTED -> throw new UnsupportedOperationException(msg);
            default -> throw new SiftException(msg);
        }
    }
}
