using System.Runtime.InteropServices;
using System.Runtime.InteropServices.Marshalling;

namespace SiftX;

/// <summary>
/// Raw P/Invoke declarations for the Sift native library.
/// Uses source-generated marshalling (LibraryImport) for best performance.
/// </summary>
internal static partial class Native
{
    private const string LibName = "siftx";

    // -----------------------------------------------------------------------
    // Result codes
    // -----------------------------------------------------------------------

    internal enum SiftResult
    {
        Ok = 0,
        InvalidArg = 1,
        IoError = 2,
        FormatError = 3,
        Truncated = 4,
        Unsupported = 5,
        InternalError = 6,
    }

    // -----------------------------------------------------------------------
    // File types
    // -----------------------------------------------------------------------

    internal enum SiftFileType
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

    // -----------------------------------------------------------------------
    // Structs (blittable, matching C layout)
    // -----------------------------------------------------------------------

    [StructLayout(LayoutKind.Sequential)]
    internal struct SiftTag
    {
        public nint Group;       // const char*
        public nint Name;        // const char*
        public nint Value;       // const char* (display string)
        public byte ValueType;   // SiftValueType discriminant
        private byte _pad0;
        private byte _pad1;
        private byte _pad2;
        public long IntVal;      // widened integer
        public int RationalNum;  // rational numerator
        public int RationalDen;  // rational denominator
        public double FloatVal;  // widened float / precomputed rational
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct SiftGps
    {
        public double Latitude;
        public double Longitude;
        public double Altitude;
        public int HasAltitude;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct SiftFormField
    {
        public nint FieldType;    // const char*
        public nint Name;         // const char*
        public nint Value;        // const char* (nullable)
        public nint DefaultValue; // const char* (nullable)
        public uint Flags;
        public int IsReadOnly;
        public int IsRequired;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct SiftAnnotation
    {
        public nint AnnotType;    // const char*
        public uint Page;         // uint32_t + 4 bytes padding
        private uint _pad0;
        public double RectLlx;
        public double RectLly;
        public double RectUrx;
        public double RectUry;
        public nint Contents;     // const char* (nullable)
        public nint Dest;         // const char* (nullable)
        public uint Flags;
        public int HasAppearance;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct SiftStructElement
    {
        public nint StructType;   // const char*
        public uint Depth;        // uint32_t + 4 bytes padding
        private uint _pad0;
        public nint Title;        // const char* (nullable)
        public nint AltText;      // const char* (nullable)
        public nint ActualText;   // const char* (nullable)
        public nint Lang;         // const char* (nullable)
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct SiftImage
    {
        public uint Page;
        public uint Width;
        public uint Height;
        public byte Bpc;
        public byte Components;
        public byte Format;
        public nint Data;     // const uint8_t*
        public nuint DataLen; // size_t
    }

    // -----------------------------------------------------------------------
    // Error handling
    // -----------------------------------------------------------------------

    [LibraryImport(LibName, EntryPoint = "siftx_error_message")]
    internal static partial nint ErrorMessage();

    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    [LibraryImport(LibName, EntryPoint = "siftx_open", StringMarshalling = StringMarshalling.Utf8)]
    internal static partial SiftResult Open(string path, out nint file);

    [LibraryImport(LibName, EntryPoint = "siftx_file_type")]
    internal static partial SiftFileType FileType(nint file);

    [LibraryImport(LibName, EntryPoint = "siftx_parse")]
    internal static partial SiftResult Parse(nint file, out nint doc);

    [LibraryImport(LibName, EntryPoint = "siftx_read")]
    internal static unsafe partial SiftResult Read(byte* data, nuint dataLen, out nint doc);

    [LibraryImport(LibName, EntryPoint = "siftx_file_free")]
    internal static partial void FileFree(nint file);

    [LibraryImport(LibName, EntryPoint = "siftx_document_free")]
    internal static partial void DocumentFree(nint doc);

    // -----------------------------------------------------------------------
    // Tags
    // -----------------------------------------------------------------------

    [LibraryImport(LibName, EntryPoint = "siftx_tags")]
    internal static partial SiftResult Tags(nint doc, out nint tags);

    [LibraryImport(LibName, EntryPoint = "siftx_tags_from_path", StringMarshalling = StringMarshalling.Utf8)]
    internal static partial SiftResult TagsFromPath(string path, out nint tags);

    [LibraryImport(LibName, EntryPoint = "siftx_tags_count")]
    internal static partial nuint TagsCount(nint tags);

    [LibraryImport(LibName, EntryPoint = "siftx_tags_get")]
    internal static partial SiftResult TagsGet(nint tags, nuint index, out SiftTag tag);

    [LibraryImport(LibName, EntryPoint = "siftx_tags_free")]
    internal static partial void TagsFree(nint tags);

    // -----------------------------------------------------------------------
    // GPS
    // -----------------------------------------------------------------------

    [LibraryImport(LibName, EntryPoint = "siftx_gps")]
    internal static partial SiftResult Gps(nint doc, out SiftGps gps);

    // -----------------------------------------------------------------------
    // Thumbnail
    // -----------------------------------------------------------------------

    [LibraryImport(LibName, EntryPoint = "siftx_thumbnail")]
    internal static partial SiftResult Thumbnail(nint doc, out nint data, out nuint len);

    [LibraryImport(LibName, EntryPoint = "siftx_thumbnail_free")]
    internal static partial void ThumbnailFree(nint data, nuint len);

    // -----------------------------------------------------------------------
    // PDF: Images
    // -----------------------------------------------------------------------

    [LibraryImport(LibName, EntryPoint = "siftx_images")]
    internal static partial SiftResult Images(nint doc, out nint images);

    [LibraryImport(LibName, EntryPoint = "siftx_images_count")]
    internal static partial nuint ImagesCount(nint images);

    [LibraryImport(LibName, EntryPoint = "siftx_images_get")]
    internal static partial SiftResult ImagesGet(nint images, nuint index, out SiftImage image);

    [LibraryImport(LibName, EntryPoint = "siftx_images_free")]
    internal static partial void ImagesFree(nint images);

    // -----------------------------------------------------------------------
    // PDF: Text
    // -----------------------------------------------------------------------

    [LibraryImport(LibName, EntryPoint = "siftx_text_pages")]
    internal static partial SiftResult TextPages(nint doc, out nint pages);

    [LibraryImport(LibName, EntryPoint = "siftx_text_pages_count")]
    internal static partial nuint TextPagesCount(nint pages);

    [LibraryImport(LibName, EntryPoint = "siftx_text_pages_get")]
    internal static partial nint TextPagesGet(nint pages, nuint index);

    [LibraryImport(LibName, EntryPoint = "siftx_text_pages_free")]
    internal static partial void TextPagesFree(nint pages);

    // -----------------------------------------------------------------------
    // Filtered tags
    // -----------------------------------------------------------------------

    [LibraryImport(LibName, EntryPoint = "siftx_exif_tags")]
    internal static partial SiftResult ExifTags(nint doc, out nint tags);

    [LibraryImport(LibName, EntryPoint = "siftx_xmp_tags")]
    internal static partial SiftResult XmpTags(nint doc, out nint tags);

    [LibraryImport(LibName, EntryPoint = "siftx_iptc_tags")]
    internal static partial SiftResult IptcTags(nint doc, out nint tags);

    // -----------------------------------------------------------------------
    // PDF: Raw text
    // -----------------------------------------------------------------------

    [LibraryImport(LibName, EntryPoint = "siftx_text_pages_raw")]
    internal static partial SiftResult TextPagesRaw(nint doc, out nint pages);

    // -----------------------------------------------------------------------
    // PDF: Authentication
    // -----------------------------------------------------------------------

    [LibraryImport(LibName, EntryPoint = "siftx_authenticate")]
    internal static partial int Authenticate(nint doc, nint password, nuint passwordLen);

    // -----------------------------------------------------------------------
    // PDF: Form fields
    // -----------------------------------------------------------------------

    [LibraryImport(LibName, EntryPoint = "siftx_form_fields")]
    internal static partial SiftResult FormFields(nint doc, out nint fields);

    [LibraryImport(LibName, EntryPoint = "siftx_form_fields_count")]
    internal static partial nuint FormFieldsCount(nint fields);

    [LibraryImport(LibName, EntryPoint = "siftx_form_fields_get")]
    internal static partial SiftResult FormFieldsGet(nint fields, nuint index, out SiftFormField field);

    [LibraryImport(LibName, EntryPoint = "siftx_form_fields_free")]
    internal static partial void FormFieldsFree(nint fields);

    // -----------------------------------------------------------------------
    // PDF: Annotations
    // -----------------------------------------------------------------------

    [LibraryImport(LibName, EntryPoint = "siftx_annotations")]
    internal static partial SiftResult Annotations(nint doc, out nint annots);

    [LibraryImport(LibName, EntryPoint = "siftx_annotations_count")]
    internal static partial nuint AnnotationsCount(nint annots);

    [LibraryImport(LibName, EntryPoint = "siftx_annotations_get")]
    internal static partial SiftResult AnnotationsGet(nint annots, nuint index, out SiftAnnotation annot);

    [LibraryImport(LibName, EntryPoint = "siftx_annotations_free")]
    internal static partial void AnnotationsFree(nint annots);

    // -----------------------------------------------------------------------
    // PDF: Structure tree
    // -----------------------------------------------------------------------

    [LibraryImport(LibName, EntryPoint = "siftx_struct_tree")]
    internal static partial SiftResult StructTree(nint doc, out nint tree);

    [LibraryImport(LibName, EntryPoint = "siftx_struct_tree_count")]
    internal static partial nuint StructTreeCount(nint tree);

    [LibraryImport(LibName, EntryPoint = "siftx_struct_tree_get")]
    internal static partial SiftResult StructTreeGet(nint tree, nuint index, out SiftStructElement elem);

    [LibraryImport(LibName, EntryPoint = "siftx_struct_tree_role_map_count")]
    internal static partial nuint StructTreeRoleMapCount(nint tree);

    [LibraryImport(LibName, EntryPoint = "siftx_struct_tree_role_map_get")]
    internal static partial SiftResult StructTreeRoleMapGet(nint tree, nuint index, out nint custom, out nint standard);

    [LibraryImport(LibName, EntryPoint = "siftx_struct_tree_free")]
    internal static partial void StructTreeFree(nint tree);

    // -----------------------------------------------------------------------
    // Document info
    // -----------------------------------------------------------------------

    [LibraryImport(LibName, EntryPoint = "siftx_document_file_type")]
    internal static partial SiftFileType DocumentFileType(nint doc);

    // -----------------------------------------------------------------------
    // Version
    // -----------------------------------------------------------------------

    [LibraryImport(LibName, EntryPoint = "siftx_version")]
    internal static partial nint Version();

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    internal static string? PtrToStringUtf8(nint ptr)
        => ptr == 0 ? null : Marshal.PtrToStringUTF8(ptr);

    internal static void ThrowOnError(SiftResult result)
    {
        if (result == SiftResult.Ok)
            return;

        var msg = PtrToStringUtf8(ErrorMessage()) ?? result.ToString();
        throw result switch
        {
            SiftResult.InvalidArg => new ArgumentException(msg),
            SiftResult.IoError => new SiftIOException(msg),
            SiftResult.FormatError => new SiftFormatException(msg),
            SiftResult.Truncated => new SiftFormatException(msg),
            SiftResult.Unsupported => new NotSupportedException(msg),
            _ => new SiftException(msg),
        };
    }
}
