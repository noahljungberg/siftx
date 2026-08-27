package com.truespar.siftx;

import org.junit.jupiter.api.*;
import java.util.*;
import static org.junit.jupiter.api.Assertions.*;
import static org.junit.jupiter.api.Assumptions.*;

class TagTest {

    @Test
    void tags_canonJpeg_hasMakeTag() throws Exception {
        assumeTrue(TestPaths.hasExifToolImages(), "testdata not available");

        try (var file = SiftFile.open(TestPaths.EXIFTOOL_IMAGES + "/Canon.jpg");
             var doc = file.parse()) {
            var tags = doc.tags();
            assertFalse(tags.isEmpty());

            var make = tags.stream().filter(t -> t.name().equals("Make")).findFirst();
            assertTrue(make.isPresent());
            assertEquals("Canon", make.get().value());
            assertEquals("EXIF", make.get().group());
        }
    }

    @Test
    void tags_canonJpeg_hasModelTag() throws Exception {
        assumeTrue(TestPaths.hasExifToolImages(), "testdata not available");

        try (var file = SiftFile.open(TestPaths.EXIFTOOL_IMAGES + "/Canon.jpg");
             var doc = file.parse()) {
            var model = doc.tags().stream()
                    .filter(t -> t.name().equals("Model"))
                    .findFirst();
            assertTrue(model.isPresent());
            assertTrue(model.get().value().contains("Canon"));
        }
    }

    @Test
    void tags_hasMultipleGroups() throws Exception {
        assumeTrue(TestPaths.hasExifToolImages(), "testdata not available");

        try (var file = SiftFile.open(TestPaths.EXIFTOOL_IMAGES + "/Canon.jpg");
             var doc = file.parse()) {
            var groups = doc.tags().stream().map(Tag::group).distinct().toList();
            assertTrue(groups.contains("EXIF"));
        }
    }

    @Test
    void tagsFromPath_convenience() throws Exception {
        assumeTrue(TestPaths.hasExifToolImages(), "testdata not available");

        var tags = SiftX.tags(TestPaths.EXIFTOOL_IMAGES + "/Canon.jpg");
        assertFalse(tags.isEmpty());
        assertTrue(tags.stream().anyMatch(t -> t.name().equals("Make")));
    }

    @Test
    void tags_scanMultipleJpegs() throws Exception {
        assumeTrue(TestPaths.hasExifToolImages(), "testdata not available");

        var jpegFiles = TestHelpers.listFiles(TestPaths.EXIFTOOL_IMAGES, "*.jpg");
        assumeFalse(jpegFiles.isEmpty(), "no JPEG files");

        int parsed = 0, withTags = 0;
        for (var path : jpegFiles.stream().limit(20).toList()) {
            try (var file = SiftFile.open(path); var doc = file.parse()) {
                parsed++;
                if (!doc.tags().isEmpty()) withTags++;
            } catch (SiftException ignored) {}
        }
        assertTrue(parsed > 0);
        assertTrue(withTags > 0);
    }

    @Test
    void tag_toString_format() {
        var tag = new Tag("EXIF", "Make", "Canon");
        assertEquals("[EXIF] Make = Canon", tag.toString());
    }

    @Test
    void tag_recordEquality() {
        var a = new Tag("EXIF", "Make", "Canon");
        var b = new Tag("EXIF", "Make", "Canon");
        var c = new Tag("EXIF", "Model", "EOS");
        assertEquals(a, b);
        assertNotEquals(a, c);
        assertEquals(a.hashCode(), b.hashCode());
    }

    @Test
    void tags_pdfHasMetadata() throws Exception {
        assumeTrue(TestPaths.hasPopplerTest(), "testdata not available");

        var paths = TestHelpers.listFilesRecursive(TestPaths.POPPLER_TEST, "*.pdf");
        assumeFalse(paths.isEmpty());

        int withTags = 0;
        for (var path : paths.stream().limit(10).toList()) {
            try {
                var tags = SiftX.tags(path);
                if (tags.stream().anyMatch(t -> t.group().equals("PDF"))) withTags++;
            } catch (SiftException ignored) {}
        }
        assertTrue(withTags > 0);
    }
}
