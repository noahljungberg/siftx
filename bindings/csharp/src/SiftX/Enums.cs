namespace SiftX;

/// <summary>
/// Raw value type discriminant from the native library.
/// String (0) means no typed value - read the display string.
/// </summary>
public enum TagValueType : byte
{
    String = 0,
    U8 = 1, U16 = 2, U32 = 3, U64 = 4,
    I8 = 5, I16 = 6, I32 = 7, I64 = 8,
    F32 = 9, F64 = 10,
    Rational = 11, SRational = 12,
}

/// <summary>
/// EXIF Orientation tag values (tag 0x0112, ISO 12234-1).
/// </summary>
public enum ExifOrientation : ushort
{
    Normal = 1,
    MirrorHorizontal = 2,
    Rotate180 = 3,
    MirrorVertical = 4,
    MirrorHorizontalRotate270 = 5,
    Rotate90 = 6,
    MirrorHorizontalRotate90 = 7,
    Rotate270 = 8,
}

/// <summary>
/// Detected file type.
/// </summary>
public enum FileType
{
    Unknown = 0,
    Jpeg = 1,
    Png = 2,
    Gif = 3,
    Bmp = 4,
    Tiff = 5,
    WebP = 6,
    Heif = 7,
    Pdf = 8,
    Icc = 9,
    QuickTime = 10,
}

/// <summary>
/// Format of an extracted PDF image.
/// </summary>
public enum ImageFormat
{
    Jpeg = 0,
    Jpeg2000 = 1,
    Jbig2 = 2,
    Ccitt = 3,
    Pixels = 4,
}

/// <summary>
/// PDF form field type (ISO 32000-2 §12.7).
/// </summary>
public enum FormFieldType
{
    Unknown,
    Text,
    Button,
    Choice,
    Signature,
}

/// <summary>
/// PDF annotation subtype (ISO 32000-2 §12.5.6).
/// </summary>
public enum AnnotationType
{
    Unknown,
    Text,
    Link,
    FreeText,
    Line,
    Square,
    Circle,
    Polygon,
    PolyLine,
    Highlight,
    Underline,
    Squiggly,
    StrikeOut,
    Stamp,
    Caret,
    Ink,
    Popup,
    FileAttachment,
    Sound,
    Movie,
    Widget,
    Screen,
    PrinterMark,
    TrapNet,
    Watermark,
    ThreeD,
    Redact,
    RichMedia,
    Projection,
}

/// <summary>
/// PDF form field flags (/Ff entry, ISO 32000-2 §12.7.2).
/// </summary>
[Flags]
public enum FormFieldFlags : uint
{
    None = 0,
    ReadOnly = 1 << 0,
    Required = 1 << 1,
    NoExport = 1 << 2,

    // --- Text field flags (§12.7.5.3) ---
    Multiline = 1 << 12,
    Password = 1 << 13,
    FileSelect = 1 << 20,
    DoNotSpellCheck = 1 << 22,
    DoNotScroll = 1 << 23,
    Comb = 1 << 24,
    RichText = 1 << 25,

    // --- Button field flags (§12.7.5.2) ---
    NoToggleToOff = 1 << 14,
    Radio = 1 << 15,
    Pushbutton = 1 << 16,
    RadiosInUnison = 1 << 25,

    // --- Choice field flags (§12.7.5.4) ---
    Combo = 1 << 17,
    Edit = 1 << 18,
    Sort = 1 << 19,
    MultiSelect = 1 << 21,
    CommitOnSelChange = 1 << 26,
}

/// <summary>
/// PDF annotation flags (/F entry, ISO 32000-2 §12.5.3).
/// </summary>
[Flags]
public enum AnnotationFlags : uint
{
    None = 0,
    Invisible = 1 << 0,
    Hidden = 1 << 1,
    Print = 1 << 2,
    NoZoom = 1 << 3,
    NoRotate = 1 << 4,
    NoView = 1 << 5,
    ReadOnly = 1 << 6,
    Locked = 1 << 7,
    ToggleNoView = 1 << 8,
    LockedContents = 1 << 9,
}
