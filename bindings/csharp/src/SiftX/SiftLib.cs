using System.Collections.Immutable;
using System.Runtime.InteropServices;

namespace SiftX;

/// <summary>
/// Static convenience methods for common Sift operations.
/// </summary>
public static class SiftLib
{
    /// <summary>
    /// Extract all metadata tags from a file in one call.
    /// This is the simplest entry point - no handles to manage.
    /// </summary>
    /// <param name="path">Path to the file.</param>
    /// <returns>Immutable array of tags.</returns>
    public static ImmutableArray<Tag> Tags(string path)
    {
        var result = Native.TagsFromPath(path, out var tagsPtr);
        Native.ThrowOnError(result);

        using var tagsHandle = new SafeTagArrayHandle(tagsPtr);
        var count = (int)Native.TagsCount(tagsPtr);
        var builder = ImmutableArray.CreateBuilder<Tag>(count);

        for (int i = 0; i < count; i++)
        {
            result = Native.TagsGet(tagsPtr, (nuint)i, out var native);
            Native.ThrowOnError(result);

            builder.Add(new Tag(
                Group: Native.PtrToStringUtf8(native.Group) ?? "",
                Name: Native.PtrToStringUtf8(native.Name) ?? "",
                Value: Native.PtrToStringUtf8(native.Value) ?? ""
            ));
        }

        return builder.MoveToImmutable();
    }

    /// <summary>
    /// Parse a document from a byte buffer.
    /// The data is copied internally - the caller can free it after this returns.
    /// </summary>
    public static unsafe SiftDocument Read(ReadOnlySpan<byte> data)
    {
        fixed (byte* ptr = data)
        {
            var result = Native.Read(ptr, (nuint)data.Length, out var docPtr);
            Native.ThrowOnError(result);
            return new SiftDocument(new SafeDocumentHandle(docPtr));
        }
    }

    /// <summary>
    /// Parse a document from a byte array.
    /// </summary>
    public static SiftDocument Read(byte[] data) => Read(data.AsSpan());

    /// <summary>
    /// Get the native library version string.
    /// </summary>
    public static string Version
    {
        get
        {
            var ptr = Native.Version();
            return Native.PtrToStringUtf8(ptr) ?? "unknown";
        }
    }
}
