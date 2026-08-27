package com.truespar.siftx;

/** Thrown when an I/O error occurs (file not found, permission denied, etc.). */
public class SiftIOException extends SiftException {
    public SiftIOException(String message) { super(message); }
}
