package com.truespar.siftx;

import java.lang.foreign.*;

/**
 * A memory-mapped file ready for parsing.
 * Implements {@link AutoCloseable} - use in try-with-resources.
 * Must outlive any {@link SiftDocument} created from it via {@link #parse()}.
 */
public final class SiftFile implements AutoCloseable {
    private final Arena arena;
    private MemorySegment handle; // native SiftXFile*

    private SiftFile(Arena arena, MemorySegment handle) {
        this.arena = arena;
        this.handle = handle;
    }

    /**
     * Open a file by path via memory-mapping.
     *
     * @param path file path
     * @return a new SiftFile handle
     * @throws SiftIOException if the file cannot be opened
     */
    public static SiftFile open(String path) {
        var arena = Arena.ofConfined();
        try {
            var pathSeg = arena.allocateFrom(path);
            var outPtr = arena.allocate(ValueLayout.ADDRESS);

            int result = (int) Native.siftx_open.invokeExact(pathSeg, outPtr);
            Native.throwOnError(result);

            var fileHandle = outPtr.get(ValueLayout.ADDRESS, 0);
            return new SiftFile(arena, fileHandle);
        } catch (SiftException e) {
            arena.close();
            throw e;
        } catch (Throwable t) {
            arena.close();
            throw new SiftException("failed to open file", t);
        }
    }

    /** Detected file type, or {@link FileType#UNKNOWN}. */
    public FileType fileType() {
        checkOpen();
        try {
            int code = (int) Native.siftx_file_type.invokeExact(handle);
            return FileType.fromCode(code);
        } catch (Throwable t) {
            throw new SiftException("failed to get file type", t);
        }
    }

    /**
     * Parse the file into a document.
     * This SiftFile must remain open for the lifetime of the returned document.
     */
    public SiftDocument parse() {
        checkOpen();
        try {
            var outPtr = arena.allocate(ValueLayout.ADDRESS);
            int result = (int) Native.siftx_parse.invokeExact(handle, outPtr);
            Native.throwOnError(result);
            var docHandle = outPtr.get(ValueLayout.ADDRESS, 0);
            return new SiftDocument(docHandle);
        } catch (SiftException e) {
            throw e;
        } catch (Throwable t) {
            throw new SiftException("failed to parse file", t);
        }
    }

    @Override
    public void close() {
        if (handle != null) {
            try {
                Native.siftx_file_free.invokeExact(handle);
            } catch (Throwable t) {
                throw new SiftException("failed to free file", t);
            }
            handle = null;
            arena.close();
        }
    }

    private void checkOpen() {
        if (handle == null) throw new IllegalStateException("SiftFile is closed");
    }
}
