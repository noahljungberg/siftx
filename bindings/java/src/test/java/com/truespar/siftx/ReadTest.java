package com.truespar.siftx;

import org.junit.jupiter.api.*;
import java.nio.file.*;
import java.util.*;
import static org.junit.jupiter.api.Assertions.*;
import static org.junit.jupiter.api.Assumptions.*;

class ReadTest {

    @Test
    void read_fromByteArray() throws Exception {
        assumeTrue(TestPaths.hasExifToolImages(), "testdata not available");

        var data = Files.readAllBytes(Path.of(TestPaths.EXIFTOOL_IMAGES, "Canon.jpg"));
        try (var doc = SiftX.read(data)) {
            assertEquals(FileType.JPEG, doc.fileType());
            var tags = doc.tags();
            assertFalse(tags.isEmpty());
            assertTrue(tags.stream().anyMatch(t -> t.name().equals("Make") && t.value().equals("Canon")));
        }
    }

    @Test
    void read_pdf_fromBuffer() throws Exception {
        assumeTrue(TestPaths.hasPopplerTest(), "testdata not available");

        var path = TestHelpers.findFirstRecursive(TestPaths.POPPLER_TEST, "*.pdf");
        assumeTrue(path != null, "no PDF files found");

        var data = Files.readAllBytes(Path.of(path));
        try (var doc = SiftX.read(data)) {
            assertEquals(FileType.PDF, doc.fileType());
        }
    }

    @Test
    void read_tags_sameAsOpen() throws Exception {
        assumeTrue(TestPaths.hasExifToolImages(), "testdata not available");

        var filePath = TestPaths.EXIFTOOL_IMAGES + "/Canon.jpg";
        var data = Files.readAllBytes(Path.of(filePath));

        List<Tag> tagsFromRead;
        try (var doc = SiftX.read(data)) {
            tagsFromRead = doc.tags();
        }

        List<Tag> tagsFromOpen;
        try (var file = SiftFile.open(filePath); var doc = file.parse()) {
            tagsFromOpen = doc.tags();
        }

        assertEquals(tagsFromOpen.size(), tagsFromRead.size());
        for (int i = 0; i < tagsFromOpen.size(); i++) {
            assertEquals(tagsFromOpen.get(i), tagsFromRead.get(i));
        }
    }

    @Test
    void read_fromSubArray() throws Exception {
        assumeTrue(TestPaths.hasExifToolImages(), "testdata not available");

        // Read into a larger buffer with offset to verify the library handles the full array
        var original = Files.readAllBytes(Path.of(TestPaths.EXIFTOOL_IMAGES, "Canon.jpg"));
        var padded = new byte[original.length + 100];
        System.arraycopy(original, 0, padded, 0, original.length);

        // Pass only the original-length portion via Arrays.copyOf
        var data = java.util.Arrays.copyOf(padded, original.length);
        try (var doc = SiftX.read(data)) {
            assertEquals(FileType.JPEG, doc.fileType());
        }
    }
}
