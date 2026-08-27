package com.truespar.siftx;

import java.util.Objects;

/**
 * A PDF form field.
 *
 * @param fieldType    Field type: "Text", "Button", "Choice", "Signature", "Unknown".
 * @param name         Fully qualified field name.
 * @param value        Current value, or null.
 * @param defaultValue Default value, or null.
 * @param flags        Field flags (/Ff).
 * @param isReadOnly   Whether the field is read-only.
 * @param isRequired   Whether the field is required.
 */
public record FormFieldInfo(
    String fieldType,
    String name,
    String value,
    String defaultValue,
    int flags,
    boolean isReadOnly,
    boolean isRequired
) {
    public FormFieldInfo {
        Objects.requireNonNull(fieldType, "fieldType");
        Objects.requireNonNull(name, "name");
    }

    @Override
    public String toString() {
        return value != null
            ? fieldType + ": " + name + " = " + value
            : fieldType + ": " + name;
    }
}
