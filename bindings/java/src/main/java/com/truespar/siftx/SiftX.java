package com.truespar.siftx;

import java.lang.foreign.*;
import java.util.*;

/**
 * Static convenience methods for common Sift operations.
 */
public final class SiftX {
    private SiftX() {}

    /**
     * Extract all metadata tags from a file in one call.
     *
     * @param path file path
     * @return unmodifiable list of tags
     */
    public static List<Tag> tags(String path) {
        try (var arena = Arena.ofConfined()) {
            var pathSeg = arena.allocateFrom(path);
            var outPtr = arena.allocate(ValueLayout.ADDRESS);

            int result = (int) Native.siftx_tags_from_path.invokeExact(pathSeg, outPtr);
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
     * Parse a document from a byte array.
     * The data is copied internally - the caller can reuse the array after this returns.
     *
     * @param data file contents
     * @return a new SiftDocument (caller must close it)
     */
    public static SiftDocument read(byte[] data) {
        try (var arena = Arena.ofConfined()) {
            var seg = arena.allocate(data.length);
            seg.copyFrom(MemorySegment.ofArray(data));

            var outPtr = arena.allocate(ValueLayout.ADDRESS);
            int result = (int) Native.siftx_read.invokeExact(seg, (long) data.length, outPtr);
            Native.throwOnError(result);

            var docHandle = outPtr.get(ValueLayout.ADDRESS, 0);
            return new SiftDocument(docHandle);
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to read data", t);
        }
    }

    /** Get the native library version string. */
    public static String version() {
        try {
            var ptr = (MemorySegment) Native.siftx_version.invokeExact();
            return Native.readUtf8(ptr);
        } catch (Throwable t) {
            throw new SiftException("failed to get version", t);
        }
    }
}
