package com.truespar.siftx;

import org.junit.jupiter.api.*;
import java.nio.file.*;
import java.util.*;
import static org.junit.jupiter.api.Assertions.*;
import static org.junit.jupiter.api.Assumptions.*;

class GpsTest {

    @Test
    void gps_fromJpegWithGps() throws Exception {
        assumeTrue(TestPaths.hasExifSamples(), "testdata not available");

        var path = TestPaths.EXIF_SAMPLES + "/jpg/gps/DSCN0010.jpg";
        assumeTrue(Files.exists(Path.of(path)), "GPS test file not found");

        try (var file = SiftFile.open(path); var doc = file.parse()) {
            var gps = doc.gps();
            assertTrue(gps.isPresent());
            assertNotEquals(0.0, gps.get().latitude());
            assertNotEquals(0.0, gps.get().longitude());
        }
    }

    @Test
    void gps_fromJpegWithoutGps_noCrash() throws Exception {
        assumeTrue(TestPaths.hasExifToolImages(), "testdata not available");

        var data = Files.readAllBytes(Path.of(TestPaths.EXIFTOOL_IMAGES, "Canon.jpg"));
        try (var doc = SiftX.read(data)) {
            // Just verify no crash - may or may not have GPS
            doc.gps();
        }
    }

    @Test
    void gps_scanExifSamples() throws Exception {
        assumeTrue(TestPaths.hasExifSamples(), "testdata not available");

        var gpsDir = TestPaths.EXIF_SAMPLES + "/jpg/gps";
        assumeTrue(Files.isDirectory(Path.of(gpsDir)), "GPS samples not found");

        int withGps = 0;
        for (var path : TestHelpers.listFiles(gpsDir, "*.jpg")) {
            try (var file = SiftFile.open(path); var doc = file.parse()) {
                if (doc.gps().isPresent()) withGps++;
            } catch (SiftException ignored) {}
        }
        assertTrue(withGps > 0, "at least some should have GPS");
    }

    @Test
    void gpsCoordinates_toString_withAltitude() {
        var gps = new GpsCoordinates(43.467157, 11.885395, OptionalDouble.of(200.5));
        assertEquals("43.467157, 11.885395, 200.5m", gps.toString());
    }

    @Test
    void gpsCoordinates_toString_withoutAltitude() {
        var gps = new GpsCoordinates(43.467157, 11.885395, OptionalDouble.empty());
        assertEquals("43.467157, 11.885395", gps.toString());
    }

    @Test
    void gpsCoordinates_recordEquality() {
        var a = new GpsCoordinates(43.5, 11.5, OptionalDouble.of(200.0));
        var b = new GpsCoordinates(43.5, 11.5, OptionalDouble.of(200.0));
        var c = new GpsCoordinates(43.5, 11.5, OptionalDouble.empty());
        assertEquals(a, b);
        assertEquals(a.hashCode(), b.hashCode());
        assertNotEquals(a, c);
    }
}
