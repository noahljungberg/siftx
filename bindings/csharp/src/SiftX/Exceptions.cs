namespace SiftX;

/// <summary>
/// Base exception for all Sift errors.
/// </summary>
public class SiftException : Exception
{
    public SiftException(string message) : base(message) { }
    public SiftException(string message, Exception inner) : base(message, inner) { }
}

/// <summary>
/// Thrown when a file cannot be opened, read, or found.
/// </summary>
public class SiftIOException : SiftException
{
    public SiftIOException(string message) : base(message) { }
}

/// <summary>
/// Thrown when a file is corrupt, truncated, or has an unrecognized format.
/// </summary>
public class SiftFormatException : SiftException
{
    public SiftFormatException(string message) : base(message) { }
}
