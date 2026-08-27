package com.truespar.siftx;

import java.io.File;
import java.nio.file.*;

final class TestPaths {
    private TestPaths() {}

    private static final String REPO_ROOT = findRepoRoot();

    static final String EXIFTOOL_IMAGES = REPO_ROOT + "/testdata/exiftool-images";
    static final String EXIF_SAMPLES = REPO_ROOT + "/testdata/exif-samples";
    static final String POPPLER_TEST = REPO_ROOT + "/testdata/poppler-test";

    static boolean hasExifToolImages() { return Files.isDirectory(Path.of(EXIFTOOL_IMAGES)); }
    static boolean hasExifSamples() { return Files.isDirectory(Path.of(EXIF_SAMPLES)); }
    static boolean hasPopplerTest() { return Files.isDirectory(Path.of(POPPLER_TEST)); }

    private static String findRepoRoot() {
        var dir = Path.of("").toAbsolutePath();
        while (dir != null) {
            if (Files.exists(dir.resolve("Cargo.toml"))) {
                return dir.toString();
            }
            dir = dir.getParent();
        }
        // Fallback
        return Path.of("").toAbsolutePath().resolve("../..").normalize().toString();
    }
}
