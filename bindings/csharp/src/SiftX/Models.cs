namespace SiftX;

/// <summary>
/// A metadata tag with group, name, and display-ready value.
/// Immutable record - zero-allocation comparison via structural equality.
/// </summary>
/// <param name="Group">Tag group: "EXIF", "XMP", "IPTC", "ICC", "PDF", "QuickTime".</param>
/// <param name="Name">Tag name: "Make", "DateCreated", etc.</param>
/// <param name="Value">Display-ready value string.</param>
public readonly record struct Tag(string Group, string Name, string Value)
{
    public override string ToString() => $"[{Group}] {Name} = {Value}";
}

/// <summary>
/// A metadata tag with typed value access.
/// The <see cref="Value"/> string is always present for display.
/// For EXIF tags, the numeric fields carry the raw parsed value so you can
/// read typed data (int, double, rational) without parsing the display string.
/// </summary>
public readonly record struct TypedTag(
    string Group,
    string Name,
    string Value,
    TagValueType ValueType,
    long IntVal,
    int RationalNum,
    int RationalDen,
    double FloatVal)
{
    /// <summary>Implicit conversion to display-only <see cref="Tag"/>.</summary>
    public static implicit operator Tag(TypedTag t) => new(t.Group, t.Name, t.Value);

    /// <summary>Get the value as a nullable int (for integer-typed tags).</summary>
    public int? AsInt32() => ValueType is >= TagValueType.U8 and <= TagValueType.I64
        ? (int)IntVal : null;

    /// <summary>Get the value as a nullable long (for integer-typed tags).</summary>
    public long? AsInt64() => ValueType is >= TagValueType.U8 and <= TagValueType.I64
        ? IntVal : null;

    /// <summary>Get the value as a nullable double (works for all numeric types).</summary>
    public double? AsDouble() => ValueType switch
    {
        TagValueType.F32 or TagValueType.F64 => FloatVal,
        TagValueType.Rational or TagValueType.SRational => FloatVal,
        >= TagValueType.U8 and <= TagValueType.I64 => (double)IntVal,
        _ => null,
    };

    /// <summary>Get rational as (numerator, denominator) tuple.</summary>
    public (int Num, int Den)? AsRational() => ValueType is TagValueType.Rational or TagValueType.SRational
        ? (RationalNum, RationalDen) : null;

    public override string ToString() => $"[{Group}] {Name} = {Value}";
}

/// <summary>
/// GPS coordinates in decimal degrees (WGS84).
/// </summary>
/// <param name="Latitude">Decimal degrees, negative = south.</param>
/// <param name="Longitude">Decimal degrees, negative = west.</param>
/// <param name="Altitude">Meters above sea level, or null if unavailable.</param>
public readonly record struct GpsCoordinates(double Latitude, double Longitude, double? Altitude)
{
    public override string ToString()
    {
        var s = FormattableString.Invariant($"{Latitude:F6}, {Longitude:F6}");
        if (Altitude.HasValue)
            s += FormattableString.Invariant($", {Altitude.Value:F1}m");
        return s;
    }
}

/// <summary>
/// A rectangle in PDF user-space coordinates.
/// </summary>
/// <param name="Llx">Lower-left X.</param>
/// <param name="Lly">Lower-left Y.</param>
/// <param name="Urx">Upper-right X.</param>
/// <param name="Ury">Upper-right Y.</param>
public readonly record struct PdfRect(double Llx, double Lly, double Urx, double Ury)
{
    /// <summary>Width of the rectangle.</summary>
    public double Width => Urx - Llx;

    /// <summary>Height of the rectangle.</summary>
    public double Height => Ury - Lly;

    public override string ToString() =>
        FormattableString.Invariant($"[{Llx:F1}, {Lly:F1}, {Urx:F1}, {Ury:F1}]");
}

/// <summary>
/// An image extracted from a PDF document.
/// Data is backed by a <see cref="ReadOnlyMemory{T}"/> for zero-copy slicing.
/// </summary>
public sealed class ExtractedImage
{
    /// <summary>0-based page index.</summary>
    public required uint Page { get; init; }

    /// <summary>Pixel width.</summary>
    public required uint Width { get; init; }

    /// <summary>Pixel height.</summary>
    public required uint Height { get; init; }

    /// <summary>Bits per component (1, 2, 4, 8, 16).</summary>
    public required byte BitsPerComponent { get; init; }

    /// <summary>Number of color components (1=gray, 3=RGB, 4=CMYK).</summary>
    public required byte Components { get; init; }

    /// <summary>Image data format.</summary>
    public required ImageFormat Format { get; init; }

    /// <summary>Raw image data bytes.</summary>
    public required ReadOnlyMemory<byte> Data { get; init; }

    /// <summary>Suggested file extension (e.g., "jpg", "jp2", "ppm").</summary>
    public string Extension => Format switch
    {
        ImageFormat.Jpeg => "jpg",
        ImageFormat.Jpeg2000 => "jp2",
        ImageFormat.Jbig2 => "jb2",
        ImageFormat.Ccitt => "tiff",
        ImageFormat.Pixels => "ppm",
        _ => "bin",
    };

    /// <summary>Whether the image data can be written directly to disk (JPEG/JP2K passthrough).</summary>
    public bool IsPassthrough => Format is ImageFormat.Jpeg or ImageFormat.Jpeg2000;
}

/// <summary>A PDF form field.</summary>
/// <param name="FieldType">Field type.</param>
/// <param name="Name">Fully qualified field name.</param>
/// <param name="Value">Current value, or null.</param>
/// <param name="DefaultValue">Default value, or null.</param>
/// <param name="Flags">Field flags (/Ff).</param>
/// <param name="IsReadOnly">True if read-only.</param>
/// <param name="IsRequired">True if required.</param>
public sealed record FormFieldInfo(
    FormFieldType FieldType,
    string Name,
    string? Value,
    string? DefaultValue,
    FormFieldFlags Flags,
    bool IsReadOnly,
    bool IsRequired);

/// <summary>A PDF annotation.</summary>
/// <param name="AnnotationType">Annotation subtype.</param>
/// <param name="Page">0-based page index.</param>
/// <param name="Rect">Bounding rectangle in PDF user-space coordinates.</param>
/// <param name="Contents">Contents text, or null.</param>
/// <param name="Destination">Destination URI (Link), or null.</param>
/// <param name="Flags">Annotation flags.</param>
/// <param name="HasAppearance">True if appearance stream exists.</param>
public sealed record AnnotationInfo(
    AnnotationType AnnotationType,
    uint Page,
    PdfRect Rect,
    string? Contents,
    string? Destination,
    AnnotationFlags Flags,
    bool HasAppearance);

/// <summary>A structure tree element (flattened depth-first).</summary>
/// <param name="StructType">Structure type: "Document", "P", "H1", "Table", etc.</param>
/// <param name="Depth">Nesting depth (0 = root).</param>
/// <param name="Title">Title, or null.</param>
/// <param name="AltText">Alternative text, or null.</param>
/// <param name="ActualText">Replacement text, or null.</param>
/// <param name="Lang">Language tag, or null.</param>
public sealed record StructElementInfo(
    string StructType,
    uint Depth,
    string? Title,
    string? AltText,
    string? ActualText,
    string? Lang);

/// <summary>A role map entry mapping custom role to standard role.</summary>
/// <param name="Custom">Custom role name.</param>
/// <param name="Standard">Standard role name it maps to.</param>
public sealed record RoleMapEntry(string Custom, string Standard);
