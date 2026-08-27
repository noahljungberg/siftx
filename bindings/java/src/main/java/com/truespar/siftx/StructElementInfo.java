package com.truespar.siftx;

import java.util.Objects;

/**
 * A tagged PDF structure element.
 *
 * @param structType  Structure type: "Document", "P", "H1", "Table", etc.
 * @param depth       Nesting depth (0 = root).
 * @param title       /T title, or null.
 * @param altText     /Alt alternative text, or null.
 * @param actualText  /ActualText replacement text, or null.
 * @param lang        /Lang language tag, or null.
 */
public record StructElementInfo(
    String structType,
    int depth,
    String title,
    String altText,
    String actualText,
    String lang
) {
    public StructElementInfo {
        Objects.requireNonNull(structType, "structType");
    }
}
