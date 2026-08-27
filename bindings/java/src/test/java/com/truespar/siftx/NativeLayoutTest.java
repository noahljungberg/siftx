package com.truespar.siftx;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

/**
 * Guards the FFM struct layouts against drift from include/siftx.h.
 *
 * These layouts size the buffers the native library writes into. A layout
 * that is too small is not a truncated read - it is a heap overflow that
 * corrupts the JVM's allocator and crashes far from the cause. TAG_LAYOUT
 * was short by 32 bytes once; nothing caught it until the JVM died inside
 * malloc. Sizes below are from the C structs in include/siftx.h on LP64.
 */
class NativeLayoutTest {

    @Test
    void tagLayout_matchesCStruct() {
        // 3 ptr + u8 + _pad[3] + 4 align + i64 + 2×i32 + f64
        assertEquals(56, Native.TAG_LAYOUT.byteSize());
    }

    @Test
    void gpsLayout_matchesCStruct() {
        // 3 × f64 + int + 4 align
        assertEquals(32, Native.GPS_LAYOUT.byteSize());
    }

    @Test
    void imageLayout_matchesCStruct() {
        // 3 × u32 + 3 × u8 + 1 align + ptr + size_t
        assertEquals(32, Native.IMAGE_LAYOUT.byteSize());
    }

    @Test
    void formFieldLayout_matchesCStruct() {
        // 4 ptr + u32 + 2 × i32 + 4 align
        assertEquals(48, Native.FORM_FIELD_LAYOUT.byteSize());
    }

    @Test
    void annotationLayout_matchesCStruct() {
        // ptr + u32 + 4 align + 4 × f64 + 2 ptr + u32 + i32
        assertEquals(72, Native.ANNOTATION_LAYOUT.byteSize());
    }

    @Test
    void structElementLayout_matchesCStruct() {
        // ptr + u32 + 4 align + 4 ptr
        assertEquals(48, Native.STRUCT_ELEMENT_LAYOUT.byteSize());
    }
}
