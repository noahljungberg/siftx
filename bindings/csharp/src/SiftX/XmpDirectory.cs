using System.Collections.Immutable;

namespace SiftX;

/// <summary>
/// Strongly-typed access to XMP metadata properties.
/// XMP properties are dynamic (any namespace can define any property), so this
/// class exposes the most common properties from standard namespaces.
/// Use the <c>this[name]</c> indexer for properties not listed here.
/// </summary>
public sealed class XmpDirectory
{
    private readonly Dictionary<string, TypedTag> _tags;

    internal XmpDirectory(ImmutableArray<TypedTag> tags)
    {
        _tags = new(StringComparer.Ordinal);
        foreach (var t in tags)
            if (t.Group is "XMP" && !_tags.ContainsKey(t.Name))
                _tags[t.Name] = t;
    }

    private string? Str(string name) => _tags.TryGetValue(name, out var t) ? t.Value : null;

    // -----------------------------------------------------------------------
    // Dublin Core (dc:)
    // -----------------------------------------------------------------------

    public string? Title => Str("Title");
    public string? Description => Str("Description");
    public string? Creator => Str("Creator");
    public string? Subject => Str("Subject");
    public string? Rights => Str("Rights");
    public string? Format => Str("Format");
    public string? Language => Str("Language");
    public string? Type => Str("Type");
    public string? Identifier => Str("Identifier");
    public string? Publisher => Str("Publisher");
    public string? Contributor => Str("Contributor");
    public string? Coverage => Str("Coverage");
    public string? Date => Str("Date");
    public string? Relation => Str("Relation");
    public string? Source => Str("Source");

    // -----------------------------------------------------------------------
    // XMP Basic (xmp:)
    // -----------------------------------------------------------------------

    public string? CreateDate => Str("CreateDate");
    public string? ModifyDate => Str("ModifyDate");
    public string? MetadataDate => Str("MetadataDate");
    public string? CreatorTool => Str("CreatorTool");
    public string? Rating => Str("Rating");
    public string? Label => Str("Label");
    public string? BaseUrl => Str("BaseURL");
    public string? Nickname => Str("Nickname");
    public string? Thumbnails => Str("Thumbnails");

    // -----------------------------------------------------------------------
    // XMP Rights (xmpRights:)
    // -----------------------------------------------------------------------

    public string? Certificate => Str("Certificate");
    public string? Marked => Str("Marked");
    public string? Owner => Str("Owner");
    public string? UsageTerms => Str("UsageTerms");
    public string? WebStatement => Str("WebStatement");

    // -----------------------------------------------------------------------
    // XMP Media Management (xmpMM:)
    // -----------------------------------------------------------------------

    public string? DocumentId => Str("DocumentID");
    public string? InstanceId => Str("InstanceID");
    public string? OriginalDocumentId => Str("OriginalDocumentID");
    public string? DerivedFrom => Str("DerivedFrom");
    public string? History => Str("History");
    public string? Ingredients => Str("Ingredients");
    public string? VersionId => Str("VersionID");
    public string? Versions => Str("Versions");
    public string? RenditionClass => Str("RenditionClass");
    public string? RenditionParams => Str("RenditionParams");
    public string? Manager => Str("Manager");
    public string? ManageTo => Str("ManageTo");
    public string? ManageFrom => Str("ManageFrom");

    // -----------------------------------------------------------------------
    // Photoshop (photoshop:)
    // -----------------------------------------------------------------------

    public string? DateCreated => Str("DateCreated");
    public string? ColorMode => Str("ColorMode");
    public string? IccProfile => Str("ICCProfile");
    public string? Country2 => Str("Country");
    public string? City => Str("City");
    public string? State => Str("State");
    public string? Headline => Str("Headline");
    public string? CaptionWriter => Str("CaptionWriter");
    public string? Category2 => Str("Category");
    public string? AuthorsPosition => Str("AuthorsPosition");
    public string? Credit2 => Str("Credit");
    public string? Source2 => Str("Source");
    public string? Instructions => Str("Instructions");
    public string? TransmissionReference => Str("TransmissionReference");
    public string? Urgency2 => Str("Urgency");
    public string? SupplementalCategories2 => Str("SupplementalCategories");

    // -----------------------------------------------------------------------
    // TIFF / EXIF in XMP (tiff:, exif:)
    // -----------------------------------------------------------------------

    public string? Make => Str("Make");
    public string? Model => Str("Model");
    public string? Orientation2 => Str("Orientation");
    public string? XResolution => Str("XResolution");
    public string? YResolution => Str("YResolution");
    public string? ResolutionUnit => Str("ResolutionUnit");
    public string? Software => Str("Software");
    public string? ImageWidth => Str("ImageWidth");
    public string? ImageLength => Str("ImageLength");
    public string? BitsPerSample => Str("BitsPerSample");
    public string? Compression => Str("Compression");
    public string? PhotometricInterpretation => Str("PhotometricInterpretation");
    public string? SamplesPerPixel => Str("SamplesPerPixel");
    public string? ExifVersion => Str("ExifVersion");
    public string? ExposureTime => Str("ExposureTime");
    public string? FNumber => Str("FNumber");
    public string? ExposureProgram => Str("ExposureProgram");
    public string? IsoSpeedRatings => Str("ISOSpeedRatings");
    public string? DateTimeOriginal => Str("DateTimeOriginal");
    public string? ShutterSpeedValue => Str("ShutterSpeedValue");
    public string? ApertureValue => Str("ApertureValue");
    public string? FocalLength => Str("FocalLength");
    public string? FocalLengthIn35mmFilm => Str("FocalLengthIn35mmFilm");
    public string? Flash => Str("Flash");
    public string? ColorSpace => Str("ColorSpace");
    public string? WhiteBalance => Str("WhiteBalance");
    public string? SceneCaptureType => Str("SceneCaptureType");
    public string? Contrast => Str("Contrast");
    public string? Saturation => Str("Saturation");
    public string? Sharpness => Str("Sharpness");
    public string? GpsLatitude => Str("GPSLatitude");
    public string? GpsLongitude => Str("GPSLongitude");
    public string? GpsAltitude => Str("GPSAltitude");
    public string? GpsTimeStamp => Str("GPSTimeStamp");

    // -----------------------------------------------------------------------
    // Camera Raw Settings (crs:)
    // -----------------------------------------------------------------------

    public string? RawFileName => Str("RawFileName");
    public string? Version => Str("Version");
    public string? ProcessVersion => Str("ProcessVersion");
    public string? WhiteBalance2 => Str("WhiteBalance");
    public string? Temperature => Str("Temperature");
    public string? Tint => Str("Tint");
    public string? Exposure => Str("Exposure");
    public string? Shadows => Str("Shadows");
    public string? Brightness => Str("Brightness");
    public string? Contrast2 => Str("Contrast");
    public string? Saturation2 => Str("Saturation");
    public string? Sharpness2 => Str("Sharpness");
    public string? LuminanceSmoothing => Str("LuminanceSmoothing");
    public string? ColorNoiseReduction => Str("ColorNoiseReduction");
    public string? VignetteAmount => Str("VignetteAmount");
    public string? ShadowTint => Str("ShadowTint");
    public string? RedHue => Str("RedHue");
    public string? RedSaturation => Str("RedSaturation");
    public string? GreenHue => Str("GreenHue");
    public string? GreenSaturation => Str("GreenSaturation");
    public string? BlueHue => Str("BlueHue");
    public string? BlueSaturation => Str("BlueSaturation");
    public string? ToneCurveName => Str("ToneCurveName");
    public string? HasSettings => Str("HasSettings");
    public string? HasCrop => Str("HasCrop");
    public string? AlreadyApplied => Str("AlreadyApplied");

    // -----------------------------------------------------------------------
    // Auxiliary (aux:)
    // -----------------------------------------------------------------------

    public string? Lens => Str("Lens");
    public string? LensId => Str("LensID");
    public string? LensInfo => Str("LensInfo");
    public string? SerialNumber => Str("SerialNumber");
    public string? Firmware => Str("Firmware");
    public string? FlashCompensation => Str("FlashCompensation");
    public string? ImageNumber => Str("ImageNumber");
    public string? ApproximateFocusDistance => Str("ApproximateFocusDistance");

    // -----------------------------------------------------------------------
    // IPTC Core (iptcCore:)
    // -----------------------------------------------------------------------

    public string? CountryCode => Str("CountryCode");
    public string? CreatorContactInfo => Str("CreatorContactInfo");
    public string? IntellectualGenre => Str("IntellectualGenre");
    public string? Location => Str("Location");
    public string? Scene => Str("Scene");
    public string? SubjectCode => Str("SubjectCode");

    // -----------------------------------------------------------------------
    // Google / HDR (Container:, HDRGainMap:)
    // -----------------------------------------------------------------------

    public string? Directory => Str("Directory");
    public string? GainMapVersion => Str("Version");
    public string? GainMapBaseRenditionIsHdr => Str("BaseRenditionIsHDR");
    public string? GainMapMin => Str("GainMapMin");
    public string? GainMapMax => Str("GainMapMax");
    public string? Gamma2 => Str("Gamma");
    public string? OffsetSdr => Str("OffsetSDR");
    public string? OffsetHdr => Str("OffsetHDR");
    public string? HdrCapacityMin => Str("HDRCapacityMin");
    public string? HdrCapacityMax => Str("HDRCapacityMax");

    /// <summary>Look up any XMP tag by name.</summary>
    public TypedTag? this[string name] => _tags.TryGetValue(name, out var t) ? t : null;
}
