using System.Collections.Immutable;
using System.Runtime.InteropServices;

namespace SiftX;

/// <summary>
/// A parsed document providing metadata tags, GPS, thumbnails, images, and text.
/// </summary>
public sealed class SiftDocument : IDisposable
{
    private readonly SafeDocumentHandle _handle;
    private bool _disposed;

    // Lazy-cached directory properties
    private ImmutableArray<TypedTag>? _typedTags;
    private ExifDirectory? _exif;
    private GpsDirectory? _gps;
    private XmpDirectory? _xmp;
    private IptcDirectory? _iptc;
    private PdfDirectory? _pdf;
    private IccDirectory? _icc;
    private QuickTimeDirectory? _quickTime;
    private HeifDirectory? _heif;
    private CompositeDirectory? _composite;
    private MakerNotesDirectory? _makerNotes;

    internal SiftDocument(SafeDocumentHandle handle)
    {
        _handle = handle;
    }

    /// <summary>Strongly-typed EXIF metadata (IFD0 + ExifIFD + Interop).</summary>
    public ExifDirectory Exif => _exif ??= new ExifDirectory(TypedTags());

    /// <summary>GPS metadata with computed decimal coordinates.</summary>
    public GpsDirectory GpsInfo => _gps ??= new GpsDirectory(TypedTags(), Gps());

    /// <summary>Strongly-typed XMP metadata.</summary>
    public XmpDirectory Xmp => _xmp ??= new XmpDirectory(TypedTags());

    /// <summary>Strongly-typed IPTC metadata.</summary>
    public IptcDirectory Iptc => _iptc ??= new IptcDirectory(TypedTags());

    /// <summary>Strongly-typed PDF document metadata.</summary>
    public PdfDirectory Pdf => _pdf ??= new PdfDirectory(TypedTags());

    /// <summary>ICC color profile metadata.</summary>
    public IccDirectory Icc => _icc ??= new IccDirectory(TypedTags());

    /// <summary>QuickTime/MP4 container metadata.</summary>
    public QuickTimeDirectory QuickTime => _quickTime ??= new QuickTimeDirectory(TypedTags());

    /// <summary>HEIF/HEIC container metadata.</summary>
    public HeifDirectory Heif => _heif ??= new HeifDirectory(TypedTags());

    /// <summary>Computed/composite metadata (derived from raw EXIF data).</summary>
    public CompositeDirectory Composite => _composite ??= new CompositeDirectory(TypedTags());

    /// <summary>Vendor-specific MakerNotes metadata.</summary>
    public MakerNotesDirectory MakerNotes => _makerNotes ??= new MakerNotesDirectory(TypedTags());

    /// <summary>Detected file type.</summary>
    public FileType FileType
    {
        get
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            return (FileType)Native.DocumentFileType(_handle.DangerousGetHandle());
        }
    }

    /// <summary>
    /// Extract all metadata tags.
    /// Returns an immutable list for thread-safe sharing.
    /// </summary>
    public ImmutableArray<Tag> Tags()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var result = Native.Tags(_handle.DangerousGetHandle(), out var tagsPtr);
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
    /// Extract all metadata tags with typed value access.
    /// EXIF tags carry raw typed values (int, float, rational);
    /// XMP/IPTC/PDF tags have display strings only.
    /// </summary>
    public ImmutableArray<TypedTag> TypedTags()
    {
        if (_typedTags.HasValue)
            return _typedTags.Value;

        ObjectDisposedException.ThrowIf(_disposed, this);

        var result = Native.Tags(_handle.DangerousGetHandle(), out var tagsPtr);
        Native.ThrowOnError(result);

        using var tagsHandle = new SafeTagArrayHandle(tagsPtr);
        var count = (int)Native.TagsCount(tagsPtr);
        var builder = ImmutableArray.CreateBuilder<TypedTag>(count);

        for (int i = 0; i < count; i++)
        {
            result = Native.TagsGet(tagsPtr, (nuint)i, out var native);
            Native.ThrowOnError(result);

            builder.Add(new TypedTag(
                Group: Native.PtrToStringUtf8(native.Group) ?? "",
                Name: Native.PtrToStringUtf8(native.Name) ?? "",
                Value: Native.PtrToStringUtf8(native.Value) ?? "",
                ValueType: (TagValueType)native.ValueType,
                IntVal: native.IntVal,
                RationalNum: native.RationalNum,
                RationalDen: native.RationalDen,
                FloatVal: native.FloatVal
            ));
        }

        _typedTags = builder.MoveToImmutable();
        return _typedTags.Value;
    }

    /// <summary>
    /// Try to extract GPS coordinates. Returns null if no GPS data.
    /// </summary>
    public GpsCoordinates? Gps()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var result = Native.Gps(_handle.DangerousGetHandle(), out var native);
        if (result == Native.SiftResult.Unsupported)
            return null;
        Native.ThrowOnError(result);

        return new GpsCoordinates(
            Latitude: native.Latitude,
            Longitude: native.Longitude,
            Altitude: native.HasAltitude != 0 ? native.Altitude : null
        );
    }

    /// <summary>
    /// Extract the EXIF thumbnail (IFD1 JPEG). Returns null if none present.
    /// The returned bytes are a complete JPEG that can be written to disk.
    /// </summary>
    public byte[]? Thumbnail()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var result = Native.Thumbnail(_handle.DangerousGetHandle(), out var dataPtr, out var len);
        if (result == Native.SiftResult.Unsupported)
            return null;
        Native.ThrowOnError(result);

        try
        {
            var bytes = new byte[(int)len];
            Marshal.Copy(dataPtr, bytes, 0, (int)len);
            return bytes;
        }
        finally
        {
            Native.ThumbnailFree(dataPtr, len);
        }
    }

    /// <summary>
    /// Extract all images from a PDF document.
    /// Returns empty for non-PDF files.
    /// Image data is copied into managed memory; the native array is freed immediately.
    /// </summary>
    public ImmutableArray<ExtractedImage> Images()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var result = Native.Images(_handle.DangerousGetHandle(), out var imagesPtr);
        Native.ThrowOnError(result);

        using var imagesHandle = new SafeImageArrayHandle(imagesPtr);
        var count = (int)Native.ImagesCount(imagesPtr);

        if (count == 0)
            return [];

        var builder = ImmutableArray.CreateBuilder<ExtractedImage>(count);

        for (int i = 0; i < count; i++)
        {
            result = Native.ImagesGet(imagesPtr, (nuint)i, out var native);
            Native.ThrowOnError(result);

            // Copy image data into managed memory
            var data = new byte[(int)native.DataLen];
            if (native.DataLen > 0)
                Marshal.Copy(native.Data, data, 0, (int)native.DataLen);

            builder.Add(new ExtractedImage
            {
                Page = native.Page,
                Width = native.Width,
                Height = native.Height,
                BitsPerComponent = native.Bpc,
                Components = native.Components,
                Format = (ImageFormat)native.Format,
                Data = data,
            });
        }

        return builder.MoveToImmutable();
    }

    /// <summary>
    /// Extract text from all PDF pages (layout-preserving).
    /// Returns empty for non-PDF files.
    /// </summary>
    public ImmutableArray<string> TextPages()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var result = Native.TextPages(_handle.DangerousGetHandle(), out var pagesPtr);
        Native.ThrowOnError(result);

        using var pagesHandle = new SafeTextPagesHandle(pagesPtr);
        var count = (int)Native.TextPagesCount(pagesPtr);

        if (count == 0)
            return [];

        var builder = ImmutableArray.CreateBuilder<string>(count);

        for (int i = 0; i < count; i++)
        {
            var textPtr = Native.TextPagesGet(pagesPtr, (nuint)i);
            builder.Add(Native.PtrToStringUtf8(textPtr) ?? "");
        }

        return builder.MoveToImmutable();
    }

    /// <summary>
    /// Extract only EXIF tags.
    /// Returns an immutable list for thread-safe sharing.
    /// </summary>
    public ImmutableArray<Tag> ExifTags()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var result = Native.ExifTags(_handle.DangerousGetHandle(), out var tagsPtr);
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
    /// Extract only XMP tags.
    /// Returns an immutable list for thread-safe sharing.
    /// </summary>
    public ImmutableArray<Tag> XmpTags()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var result = Native.XmpTags(_handle.DangerousGetHandle(), out var tagsPtr);
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
    /// Extract only IPTC tags.
    /// Returns an immutable list for thread-safe sharing.
    /// </summary>
    public ImmutableArray<Tag> IptcTags()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var result = Native.IptcTags(_handle.DangerousGetHandle(), out var tagsPtr);
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
    /// Extract raw text (no layout) per PDF page.
    /// Faster than <see cref="TextPages"/> but may lose whitespace structure.
    /// Returns empty for non-PDF files.
    /// </summary>
    public ImmutableArray<string> TextPagesRaw()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var result = Native.TextPagesRaw(_handle.DangerousGetHandle(), out var pagesPtr);
        Native.ThrowOnError(result);

        using var pagesHandle = new SafeTextPagesHandle(pagesPtr);
        var count = (int)Native.TextPagesCount(pagesPtr);

        if (count == 0)
            return [];

        var builder = ImmutableArray.CreateBuilder<string>(count);

        for (int i = 0; i < count; i++)
        {
            var textPtr = Native.TextPagesGet(pagesPtr, (nuint)i);
            builder.Add(Native.PtrToStringUtf8(textPtr) ?? "");
        }

        return builder.MoveToImmutable();
    }

    /// <summary>
    /// Authenticate an encrypted PDF with a password.
    /// Returns true if the password was accepted.
    /// </summary>
    /// <param name="password">Password bytes (UTF-8 encoded).</param>
    public bool Authenticate(ReadOnlySpan<byte> password)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        unsafe
        {
            fixed (byte* ptr = password)
            {
                return Native.Authenticate(
                    _handle.DangerousGetHandle(),
                    (nint)ptr,
                    (nuint)password.Length) != 0;
            }
        }
    }

    /// <summary>
    /// Authenticate an encrypted PDF with a string password.
    /// Returns true if the password was accepted.
    /// </summary>
    /// <param name="password">Password string (converted to UTF-8).</param>
    public bool Authenticate(string password)
    {
        return Authenticate(System.Text.Encoding.UTF8.GetBytes(password));
    }

    /// <summary>
    /// Extract PDF form fields.
    /// Returns empty for non-PDF or non-form documents.
    /// </summary>
    public ImmutableArray<FormFieldInfo> FormFields()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var result = Native.FormFields(_handle.DangerousGetHandle(), out var fieldsPtr);
        Native.ThrowOnError(result);

        using var fieldsHandle = new SafeFormFieldArrayHandle(fieldsPtr);
        var count = (int)Native.FormFieldsCount(fieldsPtr);

        if (count == 0)
            return [];

        var builder = ImmutableArray.CreateBuilder<FormFieldInfo>(count);

        for (int i = 0; i < count; i++)
        {
            result = Native.FormFieldsGet(fieldsPtr, (nuint)i, out var native);
            Native.ThrowOnError(result);

            builder.Add(new FormFieldInfo(
                FieldType: ParseFormFieldType(Native.PtrToStringUtf8(native.FieldType)),
                Name: Native.PtrToStringUtf8(native.Name) ?? "",
                Value: Native.PtrToStringUtf8(native.Value),
                DefaultValue: Native.PtrToStringUtf8(native.DefaultValue),
                Flags: (FormFieldFlags)native.Flags,
                IsReadOnly: native.IsReadOnly != 0,
                IsRequired: native.IsRequired != 0
            ));
        }

        return builder.MoveToImmutable();
    }

    /// <summary>
    /// Extract PDF annotations.
    /// Returns empty for non-PDF documents.
    /// </summary>
    public ImmutableArray<AnnotationInfo> Annotations()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var result = Native.Annotations(_handle.DangerousGetHandle(), out var annotsPtr);
        Native.ThrowOnError(result);

        using var annotsHandle = new SafeAnnotationArrayHandle(annotsPtr);
        var count = (int)Native.AnnotationsCount(annotsPtr);

        if (count == 0)
            return [];

        var builder = ImmutableArray.CreateBuilder<AnnotationInfo>(count);

        for (int i = 0; i < count; i++)
        {
            result = Native.AnnotationsGet(annotsPtr, (nuint)i, out var native);
            Native.ThrowOnError(result);

            builder.Add(new AnnotationInfo(
                AnnotationType: ParseAnnotationType(Native.PtrToStringUtf8(native.AnnotType)),
                Page: native.Page,
                Rect: new PdfRect(native.RectLlx, native.RectLly, native.RectUrx, native.RectUry),
                Contents: Native.PtrToStringUtf8(native.Contents),
                Destination: Native.PtrToStringUtf8(native.Dest),
                Flags: (AnnotationFlags)native.Flags,
                HasAppearance: native.HasAppearance != 0
            ));
        }

        return builder.MoveToImmutable();
    }

    /// <summary>
    /// Extract structure tree elements (flattened depth-first).
    /// Returns empty for non-tagged or non-PDF documents.
    /// </summary>
    public ImmutableArray<StructElementInfo> StructTree()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var result = Native.StructTree(_handle.DangerousGetHandle(), out var treePtr);
        Native.ThrowOnError(result);

        using var treeHandle = new SafeStructTreeArrayHandle(treePtr);
        var count = (int)Native.StructTreeCount(treePtr);

        if (count == 0)
            return [];

        var builder = ImmutableArray.CreateBuilder<StructElementInfo>(count);

        for (int i = 0; i < count; i++)
        {
            result = Native.StructTreeGet(treePtr, (nuint)i, out var native);
            Native.ThrowOnError(result);

            builder.Add(new StructElementInfo(
                StructType: Native.PtrToStringUtf8(native.StructType) ?? "",
                Depth: native.Depth,
                Title: Native.PtrToStringUtf8(native.Title),
                AltText: Native.PtrToStringUtf8(native.AltText),
                ActualText: Native.PtrToStringUtf8(native.ActualText),
                Lang: Native.PtrToStringUtf8(native.Lang)
            ));
        }

        return builder.MoveToImmutable();
    }

    /// <summary>
    /// Extract structure tree role map.
    /// Returns empty if no role map or non-PDF document.
    /// </summary>
    public ImmutableArray<RoleMapEntry> StructTreeRoleMap()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var result = Native.StructTree(_handle.DangerousGetHandle(), out var treePtr);
        Native.ThrowOnError(result);

        using var treeHandle = new SafeStructTreeArrayHandle(treePtr);
        var count = (int)Native.StructTreeRoleMapCount(treePtr);

        if (count == 0)
            return [];

        var builder = ImmutableArray.CreateBuilder<RoleMapEntry>(count);

        for (int i = 0; i < count; i++)
        {
            result = Native.StructTreeRoleMapGet(treePtr, (nuint)i, out var customPtr, out var standardPtr);
            Native.ThrowOnError(result);

            builder.Add(new RoleMapEntry(
                Custom: Native.PtrToStringUtf8(customPtr) ?? "",
                Standard: Native.PtrToStringUtf8(standardPtr) ?? ""
            ));
        }

        return builder.MoveToImmutable();
    }

    /// <summary>
    /// Find the first tag with the given name (case-sensitive).
    /// Returns null if not found.
    /// </summary>
    public Tag? Tag(string name)
    {
        foreach (var tag in Tags())
        {
            if (tag.Name == name)
                return tag;
        }
        return null;
    }

    /// <summary>
    /// Find the first tag with the given group and name (case-sensitive).
    /// Returns null if not found.
    /// </summary>
    public Tag? Tag(string group, string name)
    {
        foreach (var tag in Tags())
        {
            if (tag.Group == group && tag.Name == name)
                return tag;
        }
        return null;
    }

    public void Dispose()
    {
        if (!_disposed)
        {
            _disposed = true;
            _handle.Dispose();
        }
    }

    private static FormFieldType ParseFormFieldType(string? s) => s switch
    {
        "Text" => SiftX.FormFieldType.Text,
        "Button" => SiftX.FormFieldType.Button,
        "Choice" => SiftX.FormFieldType.Choice,
        "Signature" => SiftX.FormFieldType.Signature,
        _ => SiftX.FormFieldType.Unknown,
    };

    private static AnnotationType ParseAnnotationType(string? s) => s switch
    {
        "Text" => SiftX.AnnotationType.Text,
        "Link" => SiftX.AnnotationType.Link,
        "FreeText" => SiftX.AnnotationType.FreeText,
        "Line" => SiftX.AnnotationType.Line,
        "Square" => SiftX.AnnotationType.Square,
        "Circle" => SiftX.AnnotationType.Circle,
        "Polygon" => SiftX.AnnotationType.Polygon,
        "PolyLine" => SiftX.AnnotationType.PolyLine,
        "Highlight" => SiftX.AnnotationType.Highlight,
        "Underline" => SiftX.AnnotationType.Underline,
        "Squiggly" => SiftX.AnnotationType.Squiggly,
        "StrikeOut" => SiftX.AnnotationType.StrikeOut,
        "Stamp" => SiftX.AnnotationType.Stamp,
        "Caret" => SiftX.AnnotationType.Caret,
        "Ink" => SiftX.AnnotationType.Ink,
        "Popup" => SiftX.AnnotationType.Popup,
        "FileAttachment" => SiftX.AnnotationType.FileAttachment,
        "Sound" => SiftX.AnnotationType.Sound,
        "Movie" => SiftX.AnnotationType.Movie,
        "Widget" => SiftX.AnnotationType.Widget,
        "Screen" => SiftX.AnnotationType.Screen,
        "PrinterMark" => SiftX.AnnotationType.PrinterMark,
        "TrapNet" => SiftX.AnnotationType.TrapNet,
        "Watermark" => SiftX.AnnotationType.Watermark,
        "3D" => SiftX.AnnotationType.ThreeD,
        "Redact" => SiftX.AnnotationType.Redact,
        "RichMedia" => SiftX.AnnotationType.RichMedia,
        "Projection" => SiftX.AnnotationType.Projection,
        _ => SiftX.AnnotationType.Unknown,
    };
}
