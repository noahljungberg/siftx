package com.truespar.siftx;

import java.util.Objects;

/**
 * A PDF annotation.
 *
 * @param annotationType Annotation type: "Text", "Link", "Highlight", etc.
 * @param page           0-based page index.
 * @param rect           Rectangle [llx, lly, urx, ury].
 * @param contents       /Contents text, or null.
 * @param destination    Destination URI (Link), or null.
 * @param flags          Annotation flags.
 * @param hasAppearance  Whether an appearance stream exists.
 */
public record AnnotationInfo(
    String annotationType,
    int page,
    double[] rect,
    String contents,
    String destination,
    int flags,
    boolean hasAppearance
) {
    public AnnotationInfo {
        Objects.requireNonNull(annotationType, "annotationType");
        Objects.requireNonNull(rect, "rect");
    }

    @Override
    public String toString() {
        return annotationType + " on page " + (page + 1);
    }
}
