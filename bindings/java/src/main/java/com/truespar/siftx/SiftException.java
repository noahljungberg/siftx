package com.truespar.siftx;

/** Base exception for all Sift errors. */
public class SiftException extends RuntimeException {
    public SiftException(String message) { super(message); }
    public SiftException(String message, Throwable cause) { super(message, cause); }
}
