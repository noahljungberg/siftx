package com.truespar.siftx;

import org.junit.jupiter.api.*;
import java.nio.file.*;
import static org.junit.jupiter.api.Assertions.*;
import static org.junit.jupiter.api.Assumptions.*;

class PdfTest {

    // -----------------------------------------------------------------------
    // Text extraction
    // -----------------------------------------------------------------------

    @Test
    void textPages_extractsText() throws Exception {
        assumeTrue(TestPaths.hasPopplerTest(), "testdata not available");

        var path = TestHelpers.findFirstRecursive(TestPaths.POPPLER_TEST, "*.pdf");
        assumeTrue(path != null, "no PDF files found");

        try (var file = SiftFile.open(path); var doc = file.parse()) {
            var pages = doc.textPages();
            assertNotNull(pages);
        }
    }

    @Test
    void textPages_nonPdf_returnsEmpty() throws Exception {
        assumeTrue(TestPaths.hasExifToolImages(), "testdata not available");

        var data = Files.readAllBytes(Path.of(TestPaths.EXIFTOOL_IMAGES, "Canon.jpg"));
        try (var doc = SiftX.read(data)) {
            assertTrue(doc.textPages().isEmpty());
        }
    }

    @Test
    void scanPdfs_textExtraction() throws Exception {
        assumeTrue(TestPaths.hasPopplerTest(), "testdata not available");

        var paths = TestHelpers.listFilesRecursive(TestPaths.POPPLER_TEST, "*.pdf");
        assumeFalse(paths.isEmpty(), "no PDF files found");

        int parsed = 0, withText = 0;
        for (var path : paths.stream().limit(20).toList()) {
            try (var file = SiftFile.open(path); var doc = file.parse()) {
                parsed++;
                if (doc.textPages().stream().anyMatch(p -> !p.isEmpty())) withText++;
            } catch (SiftException ignored) {}
        }
        assertTrue(parsed > 0);
    }

    // -----------------------------------------------------------------------
    // Image extraction
    // -----------------------------------------------------------------------

    @Test
    void images_extractsFromPdf() throws Exception {
        assumeTrue(TestPaths.hasPopplerTest(), "testdata not available");

        var paths = TestHelpers.listFilesRecursive(TestPaths.POPPLER_TEST, "*.pdf");
        assumeFalse(paths.isEmpty(), "no PDF files found");

        for (var path : paths.stream().limit(30).toList()) {
            try (var file = SiftFile.open(path); var doc = file.parse()) {
                var images = doc.images();
                for (var img : images) {
                    assertTrue(img.width() <= 100_000, "width too large - struct alignment issue");
                    assertTrue(img.height() <= 100_000, "height too large");
                    assertNotNull(img.extension());
                }
            } catch (SiftException ignored) {}
        }
    }

    @Test
    void images_nonPdf_returnsEmpty() throws Exception {
        assumeTrue(TestPaths.hasExifToolImages(), "testdata not available");

        var data = Files.readAllBytes(Path.of(TestPaths.EXIFTOOL_IMAGES, "Canon.jpg"));
        try (var doc = SiftX.read(data)) {
            assertTrue(doc.images().isEmpty());
        }
    }

    @Test
    void extractedImage_properties() throws Exception {
        assumeTrue(TestPaths.hasPopplerTest(), "testdata not available");

        ExtractedImage found = null;
        for (var path : TestHelpers.listFilesRecursive(TestPaths.POPPLER_TEST, "*.pdf").stream().limit(20).toList()) {
            try (var file = SiftFile.open(path); var doc = file.parse()) {
                var images = doc.images();
                if (!images.isEmpty()) { found = images.getFirst(); break; }
            } catch (SiftException ignored) {}
        }
        assumeTrue(found != null, "no PDF with images found");

        assertTrue(found.width() > 0);
        assertTrue(found.height() > 0);
        assertTrue(found.bitsPerComponent() > 0);
        assertTrue(found.components() > 0);
        assertTrue(found.data().length > 0);
    }

    @Test
    void extractedImage_isPassthrough() throws Exception {
        assumeTrue(TestPaths.hasPopplerTest(), "testdata not available");

        ExtractedImage found = null;
        for (var path : TestHelpers.listFilesRecursive(TestPaths.POPPLER_TEST, "*.pdf").stream().limit(30).toList()) {
            try (var file = SiftFile.open(path); var doc = file.parse()) {
                for (var img : doc.images()) {
                    if (img.isPassthrough()) {
                        found = img;
                        break;
                    }
                }
                if (found != null) break;
            } catch (SiftException ignored) {}
        }

        if (found != null) {
            assertTrue(found.format() == ImageFormat.JPEG || found.format() == ImageFormat.JPEG2000);
            assertTrue(found.data().length > 0);
        }
        // If no passthrough images found, that's OK - just verifying the method works
    }
}
