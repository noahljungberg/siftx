package com.truespar.siftx;

import org.junit.jupiter.api.*;
import java.nio.file.*;
import static org.junit.jupiter.api.Assertions.*;
import static org.junit.jupiter.api.Assumptions.*;

class ThumbnailTest {

    @Test
    void thumbnail_dscnJpeg_returnsJpegBytes() throws Exception {
        assumeTrue(TestPaths.hasExifSamples(), "testdata not available");

        var path = TestPaths.EXIF_SAMPLES + "/jpg/gps/DSCN0010.jpg";
        assumeTrue(Files.exists(Path.of(path)), "DSCN0010.jpg not found");

        try (var file = SiftFile.open(path); var doc = file.parse()) {
            var thumb = doc.thumbnail();
            assertTrue(thumb.isPresent());
            var bytes = thumb.get();
            assertTrue(bytes.length > 100, "thumbnail should have meaningful size");
            // JPEG magic bytes
            assertEquals((byte) 0xFF, bytes[0]);
            assertEquals((byte) 0xD8, bytes[1]);
            assertEquals((byte) 0xFF, bytes[2]);
        }
    }

    @Test
    void thumbnail_endsWithJpegEoi() throws Exception {
        assumeTrue(TestPaths.hasExifSamples(), "testdata not available");

        var path = TestPaths.EXIF_SAMPLES + "/jpg/gps/DSCN0010.jpg";
        assumeTrue(Files.exists(Path.of(path)), "DSCN0010.jpg not found");

        try (var file = SiftFile.open(path); var doc = file.parse()) {
            var thumb = doc.thumbnail();
            assertTrue(thumb.isPresent());
            var bytes = thumb.get();
            // JPEG EOI marker
            assertEquals((byte) 0xFF, bytes[bytes.length - 2]);
            assertEquals((byte) 0xD9, bytes[bytes.length - 1]);
        }
    }

    @Test
    void thumbnail_scanJpegs_someHaveThumbnails() throws Exception {
        assumeTrue(TestPaths.hasExifSamples(), "testdata not available");

        var jpegDir = TestPaths.EXIF_SAMPLES + "/jpg";
        assumeTrue(Files.isDirectory(Path.of(jpegDir)));

        int withThumb = 0;
        var files = TestHelpers.listFilesRecursive(jpegDir, "*.jpg");
        for (var path : files) {
            try (var file = SiftFile.open(path); var doc = file.parse()) {
                if (doc.thumbnail().isPresent()) withThumb++;
            } catch (SiftException ignored) {}
        }
        assertTrue(withThumb > 0, "at least some JPEGs should have thumbnails");
    }

    @Test
    void thumbnail_pdf_returnsEmpty() throws Exception {
        assumeTrue(TestPaths.hasPopplerTest(), "testdata not available");

        var path = TestHelpers.findFirstRecursive(TestPaths.POPPLER_TEST, "*.pdf");
        assumeTrue(path != null, "no PDF files found");

        try (var file = SiftFile.open(path); var doc = file.parse()) {
            assertTrue(doc.thumbnail().isEmpty());
        }
    }

    @Test
    void thumbnail_noThumbnail_noCrash() throws Exception {
        assumeTrue(TestPaths.hasExifToolImages(), "testdata not available");

        try (var file = SiftFile.open(TestPaths.EXIFTOOL_IMAGES + "/Canon.jpg");
             var doc = file.parse()) {
            // Just verify no crash - may or may not have thumbnail
            doc.thumbnail();
        }
    }
}
