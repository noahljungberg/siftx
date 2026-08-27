package com.truespar.siftx;

import java.util.Objects;

/**
 * A metadata tag with group, name, and display-ready value.
 *
 * @param group Tag group: "EXIF", "XMP", "IPTC", "ICC", "PDF", "QuickTime".
 * @param name  Tag name: "Make", "DateCreated", etc.
 * @param value Display-ready value string.
 */
public record Tag(String group, String name, String value) {
    public Tag {
        Objects.requireNonNull(group);
        Objects.requireNonNull(name);
        Objects.requireNonNull(value);
    }

    @Override
    public String toString() {
        return "[" + group + "] " + name + " = " + value;
    }
}
