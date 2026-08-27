package com.truespar.siftx;

import org.junit.jupiter.api.*;
import static org.junit.jupiter.api.Assertions.*;

class VersionTest {

    @Test
    void version_returnsValidString() {
        var version = SiftX.version();
        assertNotNull(version);
        assertEquals("0.1.0", version);
    }
}
