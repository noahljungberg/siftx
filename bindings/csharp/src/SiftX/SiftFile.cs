namespace SiftX;

/// <summary>
/// A memory-mapped file ready for parsing.
/// Dispose this only after all <see cref="SiftDocument"/> instances created from it.
/// </summary>
public sealed class SiftFile : IDisposable
{
    private readonly SafeFileHandle _handle;
    private bool _disposed;

    private SiftFile(SafeFileHandle handle)
    {
        _handle = handle;
    }

    /// <summary>
    /// Open a file by path via memory-mapping.
    /// </summary>
    /// <exception cref="IOException">File not found or I/O error.</exception>
    /// <exception cref="SiftFormatException">Corrupt or unrecognized file.</exception>
    public static SiftFile Open(string path)
    {
        var result = Native.Open(path, out var ptr);
        Native.ThrowOnError(result);
        return new SiftFile(new SafeFileHandle(ptr));
    }

    /// <summary>Detected file type, or <see cref="FileType.Unknown"/>.</summary>
    public FileType FileType
    {
        get
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            return (FileType)Native.FileType(_handle.DangerousGetHandle());
        }
    }

    /// <summary>
    /// Parse the file into a document.
    /// The returned document borrows from this file - keep this alive.
    /// </summary>
    public SiftDocument Parse()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        var result = Native.Parse(_handle.DangerousGetHandle(), out var docPtr);
        Native.ThrowOnError(result);
        return new SiftDocument(new SafeDocumentHandle(docPtr));
    }

    public void Dispose()
    {
        if (!_disposed)
        {
            _disposed = true;
            _handle.Dispose();
        }
    }
}
