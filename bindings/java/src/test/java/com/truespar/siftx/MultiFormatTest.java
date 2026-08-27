package com.truespar.siftx;

import org.junit.jupiter.api.*;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.*;
import static org.junit.jupiter.api.Assertions.*;
import static org.junit.jupiter.api.Assumptions.*;

class MultiFormatTest {

    @Test
    void scanAllFormats_noExceptions() throws Exception {
        assumeTrue(TestPaths.hasExifToolImages(), "testdata not available");

        var extensions = List.of("*.jpg", "*.png", "*.gif", "*.bmp", "*.tif", "*.tiff", "*.webp");
        int total = 0, parsed = 0;

        for (var ext : extensions) {
            for (var path : TestHelpers.listFiles(TestPaths.EXIFTOOL_IMAGES, ext).stream().limit(5).toList()) {
                total++;
                try (var file = SiftFile.open(path); var doc = file.parse()) {
                    doc.tags();
                    parsed++;
                } catch (SiftException ignored) {}
            }
        }
        assertTrue(total > 0);
        assertTrue(parsed > 0);
    }

    @Test
    void readFromBuffer_allFormats() throws Exception {
        assumeTrue(TestPaths.hasExifToolImages(), "testdata not available");

        record Expect(String glob, FileType type) {}
        var cases = List.of(
                new Expect("*.jpg", FileType.JPEG),
                new Expect("*.png", FileType.PNG),
                new Expect("*.gif", FileType.GIF)
        );

        for (var c : cases) {
            var path = TestHelpers.findFirst(TestPaths.EXIFTOOL_IMAGES, c.glob());
            if (path == null) continue;
            var data = Files.readAllBytes(Path.of(path));
            try (var doc = SiftX.read(data)) {
                assertEquals(c.type(), doc.fileType());
            }
        }
    }

    @Test
    void tags_unmodifiableList_isThreadSafe() throws Exception {
        assumeTrue(TestPaths.hasExifToolImages(), "testdata not available");

        var data = Files.readAllBytes(Path.of(TestPaths.EXIFTOOL_IMAGES, "Canon.jpg"));
        List<Tag> tags;
        try (var doc = SiftX.read(data)) {
            tags = doc.tags();
        }

        // Unmodifiable list can be safely shared across threads
        var executor = Executors.newFixedThreadPool(4);
        var futures = new ArrayList<Future<Integer>>();

        for (int i = 0; i < 4; i++) {
            futures.add(executor.submit(() -> {
                for (var tag : tags) {
                    var s = tag.toString();
                    assertNotNull(s);
                }
                return tags.size();
            }));
        }

        for (var future : futures) {
            assertEquals(tags.size(), future.get(5, TimeUnit.SECONDS));
        }

        executor.shutdown();
        assertTrue(executor.awaitTermination(5, TimeUnit.SECONDS));
    }
}
