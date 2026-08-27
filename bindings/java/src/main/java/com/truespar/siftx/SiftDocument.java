package com.truespar.siftx;

import java.lang.foreign.*;
import java.util.*;

/**
 * A parsed document providing metadata tags, GPS, thumbnails, images, and text.
 * Implements {@link AutoCloseable} - use in try-with-resources.
 */
public final class SiftDocument implements AutoCloseable {
    private MemorySegment handle; // native SiftXDocument*

    SiftDocument(MemorySegment handle) {
        this.handle = handle;
    }

    /** Detected file type. */
    public FileType fileType() {
        checkOpen();
        try {
            int code = (int) Native.siftx_document_file_type.invokeExact(handle);
            return FileType.fromCode(code);
        } catch (Throwable t) {
            throw new SiftException("failed to get file type", t);
        }
    }

    /**
     * Extract all metadata tags.
     * Returns an unmodifiable list.
     */
    public List<Tag> tags() {
        checkOpen();
        try (var arena = Arena.ofConfined()) {
            var outPtr = arena.allocate(ValueLayout.ADDRESS);
            int result = (int) Native.siftx_tags.invokeExact(handle, outPtr);
            Native.throwOnError(result);

            var tagsHandle = outPtr.get(ValueLayout.ADDRESS, 0);
            try {
                long count = (long) Native.siftx_tags_count.invokeExact(tagsHandle);
                var tags = new ArrayList<Tag>((int) count);

                var tagBuf = arena.allocate(Native.TAG_LAYOUT);
                for (long i = 0; i < count; i++) {
                    result = (int) Native.siftx_tags_get.invokeExact(tagsHandle, i, tagBuf);
                    Native.throwOnError(result);

                    var group = Native.readUtf8(tagBuf.get(ValueLayout.ADDRESS, 0));
                    var name = Native.readUtf8(tagBuf.get(ValueLayout.ADDRESS, 8));
                    var value = Native.readUtf8(tagBuf.get(ValueLayout.ADDRESS, 16));
                    tags.add(new Tag(
                            group != null ? group : "",
                            name != null ? name : "",
                            value != null ? value : ""
                    ));
                }
                return Collections.unmodifiableList(tags);
            } finally {
                Native.siftx_tags_free.invokeExact(tagsHandle);
            }
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to extract tags", t);
        }
    }

    /**
     * Try to extract GPS coordinates.
     * Returns empty if no GPS data is present.
     */
    public Optional<GpsCoordinates> gps() {
        checkOpen();
        try (var arena = Arena.ofConfined()) {
            var gpsBuf = arena.allocate(Native.GPS_LAYOUT);
            int result = (int) Native.siftx_gps.invokeExact(handle, gpsBuf);
            if (result == Native.UNSUPPORTED) return Optional.empty();
            Native.throwOnError(result);

            double lat = gpsBuf.get(ValueLayout.JAVA_DOUBLE, 0);
            double lon = gpsBuf.get(ValueLayout.JAVA_DOUBLE, 8);
            double alt = gpsBuf.get(ValueLayout.JAVA_DOUBLE, 16);
            int hasAlt = gpsBuf.get(ValueLayout.JAVA_INT, 24);

            return Optional.of(new GpsCoordinates(lat, lon,
                    hasAlt != 0 ? OptionalDouble.of(alt) : OptionalDouble.empty()));
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to extract GPS", t);
        }
    }

    /**
     * Extract the EXIF thumbnail (IFD1 JPEG).
     * Returns empty if no thumbnail is present.
     * The returned bytes are a complete JPEG that can be written to disk.
     */
    public Optional<byte[]> thumbnail() {
        checkOpen();
        try (var arena = Arena.ofConfined()) {
            var dataPtr = arena.allocate(ValueLayout.ADDRESS);
            var lenPtr = arena.allocate(ValueLayout.JAVA_LONG);

            int result = (int) Native.siftx_thumbnail.invokeExact(handle, dataPtr, lenPtr);
            if (result == Native.UNSUPPORTED) return Optional.empty();
            Native.throwOnError(result);

            var data = dataPtr.get(ValueLayout.ADDRESS, 0);
            long len = lenPtr.get(ValueLayout.JAVA_LONG, 0);

            try {
                var bytes = data.reinterpret(len).toArray(ValueLayout.JAVA_BYTE);
                return Optional.of(bytes);
            } finally {
                Native.siftx_thumbnail_free.invokeExact(data, len);
            }
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to extract thumbnail", t);
        }
    }

    /**
     * Extract all images from a PDF document.
     * Returns empty list for non-PDF files.
     * Image data is copied into Java byte arrays.
     */
    public List<ExtractedImage> images() {
        checkOpen();
        try (var arena = Arena.ofConfined()) {
            var outPtr = arena.allocate(ValueLayout.ADDRESS);
            int result = (int) Native.siftx_images.invokeExact(handle, outPtr);
            Native.throwOnError(result);

            var imagesHandle = outPtr.get(ValueLayout.ADDRESS, 0);
            try {
                long count = (long) Native.siftx_images_count.invokeExact(imagesHandle);
                if (count == 0) return List.of();

                var images = new ArrayList<ExtractedImage>((int) count);
                var imgBuf = arena.allocate(Native.IMAGE_LAYOUT);

                for (long i = 0; i < count; i++) {
                    result = (int) Native.siftx_images_get.invokeExact(imagesHandle, i, imgBuf);
                    Native.throwOnError(result);

                    int page = imgBuf.get(ValueLayout.JAVA_INT, 0);
                    int width = imgBuf.get(ValueLayout.JAVA_INT, 4);
                    int height = imgBuf.get(ValueLayout.JAVA_INT, 8);
                    int bpc = Byte.toUnsignedInt(imgBuf.get(ValueLayout.JAVA_BYTE, 12));
                    int components = Byte.toUnsignedInt(imgBuf.get(ValueLayout.JAVA_BYTE, 13));
                    int format = Byte.toUnsignedInt(imgBuf.get(ValueLayout.JAVA_BYTE, 14));
                    var dataPtr = imgBuf.get(ValueLayout.ADDRESS, 16);
                    long dataLen = imgBuf.get(ValueLayout.JAVA_LONG, 24);

                    byte[] data = dataLen > 0
                            ? dataPtr.reinterpret(dataLen).toArray(ValueLayout.JAVA_BYTE)
                            : new byte[0];

                    images.add(new ExtractedImage(page, width, height, bpc, components,
                            ImageFormat.fromCode(format), data));
                }
                return Collections.unmodifiableList(images);
            } finally {
                Native.siftx_images_free.invokeExact(imagesHandle);
            }
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to extract images", t);
        }
    }

    /**
     * Extract text from all PDF pages (layout-preserving).
     * Returns empty list for non-PDF files.
     */
    public List<String> textPages() {
        checkOpen();
        try (var arena = Arena.ofConfined()) {
            var outPtr = arena.allocate(ValueLayout.ADDRESS);
            int result = (int) Native.siftx_text_pages.invokeExact(handle, outPtr);
            Native.throwOnError(result);

            var pagesHandle = outPtr.get(ValueLayout.ADDRESS, 0);
            try {
                long count = (long) Native.siftx_text_pages_count.invokeExact(pagesHandle);
                if (count == 0) return List.of();

                var pages = new ArrayList<String>((int) count);
                for (long i = 0; i < count; i++) {
                    var textPtr = (MemorySegment) Native.siftx_text_pages_get.invokeExact(pagesHandle, i);
                    var text = Native.readUtf8(textPtr);
                    pages.add(text != null ? text : "");
                }
                return Collections.unmodifiableList(pages);
            } finally {
                Native.siftx_text_pages_free.invokeExact(pagesHandle);
            }
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to extract text pages", t);
        }
    }

    /**
     * Extract only EXIF metadata tags.
     * Returns an unmodifiable list.
     */
    public List<Tag> exifTags() {
        checkOpen();
        try (var arena = Arena.ofConfined()) {
            var outPtr = arena.allocate(ValueLayout.ADDRESS);
            int result = (int) Native.siftx_exif_tags.invokeExact(handle, outPtr);
            Native.throwOnError(result);
            return readTagArray(arena, outPtr.get(ValueLayout.ADDRESS, 0));
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to extract EXIF tags", t);
        }
    }

    /**
     * Extract only XMP metadata tags.
     * Returns an unmodifiable list.
     */
    public List<Tag> xmpTags() {
        checkOpen();
        try (var arena = Arena.ofConfined()) {
            var outPtr = arena.allocate(ValueLayout.ADDRESS);
            int result = (int) Native.siftx_xmp_tags.invokeExact(handle, outPtr);
            Native.throwOnError(result);
            return readTagArray(arena, outPtr.get(ValueLayout.ADDRESS, 0));
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to extract XMP tags", t);
        }
    }

    /**
     * Extract only IPTC metadata tags.
     * Returns an unmodifiable list.
     */
    public List<Tag> iptcTags() {
        checkOpen();
        try (var arena = Arena.ofConfined()) {
            var outPtr = arena.allocate(ValueLayout.ADDRESS);
            int result = (int) Native.siftx_iptc_tags.invokeExact(handle, outPtr);
            Native.throwOnError(result);
            return readTagArray(arena, outPtr.get(ValueLayout.ADDRESS, 0));
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to extract IPTC tags", t);
        }
    }

    /** Read tags from a SiftXTagArray handle, then free it. */
    private List<Tag> readTagArray(Arena arena, MemorySegment tagsHandle) throws Throwable {
        try {
            long count = (long) Native.siftx_tags_count.invokeExact(tagsHandle);
            var tags = new ArrayList<Tag>((int) count);

            var tagBuf = arena.allocate(Native.TAG_LAYOUT);
            for (long i = 0; i < count; i++) {
                int result = (int) Native.siftx_tags_get.invokeExact(tagsHandle, i, tagBuf);
                Native.throwOnError(result);

                var group = Native.readUtf8(tagBuf.get(ValueLayout.ADDRESS, 0));
                var name = Native.readUtf8(tagBuf.get(ValueLayout.ADDRESS, 8));
                var value = Native.readUtf8(tagBuf.get(ValueLayout.ADDRESS, 16));
                tags.add(new Tag(
                        group != null ? group : "",
                        name != null ? name : "",
                        value != null ? value : ""
                ));
            }
            return Collections.unmodifiableList(tags);
        } finally {
            Native.siftx_tags_free.invokeExact(tagsHandle);
        }
    }

    /**
     * Extract raw text (no layout) from all PDF pages.
     * Faster than {@link #textPages()} but may lose whitespace structure.
     * Returns empty list for non-PDF files.
     */
    public List<String> textPagesRaw() {
        checkOpen();
        try (var arena = Arena.ofConfined()) {
            var outPtr = arena.allocate(ValueLayout.ADDRESS);
            int result = (int) Native.siftx_text_pages_raw.invokeExact(handle, outPtr);
            Native.throwOnError(result);

            var pagesHandle = outPtr.get(ValueLayout.ADDRESS, 0);
            try {
                long count = (long) Native.siftx_text_pages_count.invokeExact(pagesHandle);
                if (count == 0) return List.of();

                var pages = new ArrayList<String>((int) count);
                for (long i = 0; i < count; i++) {
                    var textPtr = (MemorySegment) Native.siftx_text_pages_get.invokeExact(pagesHandle, i);
                    var text = Native.readUtf8(textPtr);
                    pages.add(text != null ? text : "");
                }
                return Collections.unmodifiableList(pages);
            } finally {
                Native.siftx_text_pages_free.invokeExact(pagesHandle);
            }
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to extract raw text pages", t);
        }
    }

    /**
     * Authenticate an encrypted PDF with a password.
     *
     * @param password Password bytes.
     * @return true if accepted, false if wrong or not encrypted.
     */
    public boolean authenticate(byte[] password) {
        checkOpen();
        try (var arena = Arena.ofConfined()) {
            var pwSeg = arena.allocate(password.length);
            pwSeg.copyFrom(MemorySegment.ofArray(password));
            int result = (int) Native.siftx_authenticate.invokeExact(handle, pwSeg, (long) password.length);
            return result == 1;
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to authenticate", t);
        }
    }

    /**
     * Authenticate an encrypted PDF with a password string.
     *
     * @param password Password (encoded as UTF-8).
     * @return true if accepted, false if wrong or not encrypted.
     */
    public boolean authenticate(String password) {
        return authenticate(password.getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }

    /**
     * Extract form fields from a PDF document.
     * Returns empty list for non-PDF/non-form documents.
     */
    public List<FormFieldInfo> formFields() {
        checkOpen();
        try (var arena = Arena.ofConfined()) {
            var outPtr = arena.allocate(ValueLayout.ADDRESS);
            int result = (int) Native.siftx_form_fields.invokeExact(handle, outPtr);
            Native.throwOnError(result);

            var fieldsHandle = outPtr.get(ValueLayout.ADDRESS, 0);
            try {
                long count = (long) Native.siftx_form_fields_count.invokeExact(fieldsHandle);
                if (count == 0) return List.of();

                var fields = new ArrayList<FormFieldInfo>((int) count);
                var buf = arena.allocate(Native.FORM_FIELD_LAYOUT);

                for (long i = 0; i < count; i++) {
                    result = (int) Native.siftx_form_fields_get.invokeExact(fieldsHandle, i, buf);
                    Native.throwOnError(result);

                    var fieldType = Native.readUtf8(buf.get(ValueLayout.ADDRESS, 0));
                    var name = Native.readUtf8(buf.get(ValueLayout.ADDRESS, 8));
                    var value = Native.readUtf8(buf.get(ValueLayout.ADDRESS, 16));
                    var defaultValue = Native.readUtf8(buf.get(ValueLayout.ADDRESS, 24));
                    int flags = buf.get(ValueLayout.JAVA_INT, 32);
                    int isReadOnly = buf.get(ValueLayout.JAVA_INT, 36);
                    int isRequired = buf.get(ValueLayout.JAVA_INT, 40);

                    fields.add(new FormFieldInfo(
                            fieldType != null ? fieldType : "",
                            name != null ? name : "",
                            value,
                            defaultValue,
                            flags,
                            isReadOnly != 0,
                            isRequired != 0
                    ));
                }
                return Collections.unmodifiableList(fields);
            } finally {
                Native.siftx_form_fields_free.invokeExact(fieldsHandle);
            }
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to extract form fields", t);
        }
    }

    /**
     * Extract all annotations from a PDF document.
     * Returns empty list for non-PDF documents.
     */
    public List<AnnotationInfo> annotations() {
        checkOpen();
        try (var arena = Arena.ofConfined()) {
            var outPtr = arena.allocate(ValueLayout.ADDRESS);
            int result = (int) Native.siftx_annotations.invokeExact(handle, outPtr);
            Native.throwOnError(result);

            var annotsHandle = outPtr.get(ValueLayout.ADDRESS, 0);
            try {
                long count = (long) Native.siftx_annotations_count.invokeExact(annotsHandle);
                if (count == 0) return List.of();

                var annots = new ArrayList<AnnotationInfo>((int) count);
                var buf = arena.allocate(Native.ANNOTATION_LAYOUT);

                for (long i = 0; i < count; i++) {
                    result = (int) Native.siftx_annotations_get.invokeExact(annotsHandle, i, buf);
                    Native.throwOnError(result);

                    var annotType = Native.readUtf8(buf.get(ValueLayout.ADDRESS, 0));
                    int page = buf.get(ValueLayout.JAVA_INT, 8);
                    // padding at 12-15
                    double llx = buf.get(ValueLayout.JAVA_DOUBLE, 16);
                    double lly = buf.get(ValueLayout.JAVA_DOUBLE, 24);
                    double urx = buf.get(ValueLayout.JAVA_DOUBLE, 32);
                    double ury = buf.get(ValueLayout.JAVA_DOUBLE, 40);
                    var contents = Native.readUtf8(buf.get(ValueLayout.ADDRESS, 48));
                    var dest = Native.readUtf8(buf.get(ValueLayout.ADDRESS, 56));
                    int flags = buf.get(ValueLayout.JAVA_INT, 64);
                    int hasAppearance = buf.get(ValueLayout.JAVA_INT, 68);

                    annots.add(new AnnotationInfo(
                            annotType != null ? annotType : "",
                            page,
                            new double[]{llx, lly, urx, ury},
                            contents,
                            dest,
                            flags,
                            hasAppearance != 0
                    ));
                }
                return Collections.unmodifiableList(annots);
            } finally {
                Native.siftx_annotations_free.invokeExact(annotsHandle);
            }
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to extract annotations", t);
        }
    }

    /**
     * Extract the tagged structure tree from a PDF.
     * Elements are flattened depth-first with depth markers.
     * Returns empty list for non-tagged/non-PDF documents.
     */
    public List<StructElementInfo> structTree() {
        checkOpen();
        try (var arena = Arena.ofConfined()) {
            var outPtr = arena.allocate(ValueLayout.ADDRESS);
            int result = (int) Native.siftx_struct_tree.invokeExact(handle, outPtr);
            Native.throwOnError(result);

            var treeHandle = outPtr.get(ValueLayout.ADDRESS, 0);
            try {
                long count = (long) Native.siftx_struct_tree_count.invokeExact(treeHandle);
                if (count == 0) return List.of();

                var elements = new ArrayList<StructElementInfo>((int) count);
                var buf = arena.allocate(Native.STRUCT_ELEMENT_LAYOUT);

                for (long i = 0; i < count; i++) {
                    result = (int) Native.siftx_struct_tree_get.invokeExact(treeHandle, i, buf);
                    Native.throwOnError(result);

                    var structType = Native.readUtf8(buf.get(ValueLayout.ADDRESS, 0));
                    int depth = buf.get(ValueLayout.JAVA_INT, 8);
                    // padding at 12-15
                    var title = Native.readUtf8(buf.get(ValueLayout.ADDRESS, 16));
                    var altText = Native.readUtf8(buf.get(ValueLayout.ADDRESS, 24));
                    var actualText = Native.readUtf8(buf.get(ValueLayout.ADDRESS, 32));
                    var lang = Native.readUtf8(buf.get(ValueLayout.ADDRESS, 40));

                    elements.add(new StructElementInfo(
                            structType != null ? structType : "",
                            depth,
                            title,
                            altText,
                            actualText,
                            lang
                    ));
                }
                return Collections.unmodifiableList(elements);
            } finally {
                Native.siftx_struct_tree_free.invokeExact(treeHandle);
            }
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to extract structure tree", t);
        }
    }

    /**
     * Extract the role map from the tagged structure tree.
     * Returns empty list if no role map or non-tagged/non-PDF document.
     */
    public List<RoleMapEntry> structTreeRoleMap() {
        checkOpen();
        try (var arena = Arena.ofConfined()) {
            var outPtr = arena.allocate(ValueLayout.ADDRESS);
            int result = (int) Native.siftx_struct_tree.invokeExact(handle, outPtr);
            Native.throwOnError(result);

            var treeHandle = outPtr.get(ValueLayout.ADDRESS, 0);
            try {
                long count = (long) Native.siftx_struct_tree_role_map_count.invokeExact(treeHandle);
                if (count == 0) return List.of();

                var entries = new ArrayList<RoleMapEntry>((int) count);
                var customPtr = arena.allocate(ValueLayout.ADDRESS);
                var standardPtr = arena.allocate(ValueLayout.ADDRESS);

                for (long i = 0; i < count; i++) {
                    result = (int) Native.siftx_struct_tree_role_map_get.invokeExact(
                            treeHandle, i, customPtr, standardPtr);
                    Native.throwOnError(result);

                    var custom = Native.readUtf8(customPtr.get(ValueLayout.ADDRESS, 0));
                    var standard = Native.readUtf8(standardPtr.get(ValueLayout.ADDRESS, 0));

                    entries.add(new RoleMapEntry(
                            custom != null ? custom : "",
                            standard != null ? standard : ""
                    ));
                }
                return Collections.unmodifiableList(entries);
            } finally {
                Native.siftx_struct_tree_free.invokeExact(treeHandle);
            }
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to extract structure tree role map", t);
        }
    }

    @Override
    public void close() {
        if (handle != null) {
            try {
                Native.siftx_document_free.invokeExact(handle);
            } catch (Throwable t) {
                throw new SiftException("failed to free document", t);
            }
            handle = null;
        }
    }

    private void checkOpen() {
        if (handle == null) throw new IllegalStateException("SiftDocument is closed");
    }
}
