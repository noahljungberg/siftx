package com.truespar.siftx;

import java.io.*;
import java.nio.file.*;
import java.util.*;

final class TestHelpers {
    private TestHelpers() {}

    static String findFirst(String dir, String glob) throws IOException {
        try (var stream = Files.newDirectoryStream(Path.of(dir), glob)) {
            var it = stream.iterator();
            return it.hasNext() ? it.next().toString() : null;
        }
    }

    static String findFirstRecursive(String dir, String glob) throws IOException {
        try (var stream = Files.walk(Path.of(dir))) {
            var matcher = FileSystems.getDefault().getPathMatcher("glob:" + glob);
            return stream.filter(p -> matcher.matches(p.getFileName()))
                    .map(Path::toString)
                    .findFirst()
                    .orElse(null);
        }
    }

    static List<String> listFiles(String dir, String glob) throws IOException {
        try (var stream = Files.newDirectoryStream(Path.of(dir), glob)) {
            var list = new ArrayList<String>();
            stream.forEach(p -> list.add(p.toString()));
            return list;
        }
    }

    static List<String> listFilesRecursive(String dir, String glob) throws IOException {
        var matcher = FileSystems.getDefault().getPathMatcher("glob:" + glob);
        try (var stream = Files.walk(Path.of(dir))) {
            return stream.filter(p -> matcher.matches(p.getFileName()))
                    .map(Path::toString)
                    .toList();
        }
    }
}
