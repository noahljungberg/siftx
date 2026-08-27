package com.truespar.siftx;

/** Thrown when a file is corrupt, truncated, or has an unrecognized format. */
public class SiftFormatException extends SiftException {
    public SiftFormatException(String message) { super(message); }
}
