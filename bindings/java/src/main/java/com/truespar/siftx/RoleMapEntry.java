package com.truespar.siftx;

import java.util.Objects;

/**
 * A role map entry mapping a custom structure type to a standard one.
 *
 * @param custom   Custom structure type name.
 * @param standard Standard structure type it maps to.
 */
public record RoleMapEntry(String custom, String standard) {
    public RoleMapEntry {
        Objects.requireNonNull(custom, "custom");
        Objects.requireNonNull(standard, "standard");
    }
}
