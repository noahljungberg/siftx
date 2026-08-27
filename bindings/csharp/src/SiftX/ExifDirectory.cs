using System.Collections.Immutable;

namespace SiftX;

/// <summary>
/// Strongly-typed access to EXIF metadata (IFD0 + ExifIFD + Interop).
/// Every standard EXIF tag defined in the library is exposed as a property.
/// For tags not listed, use the <c>this[name]</c> indexer.
/// </summary>
public sealed class ExifDirectory
{
    private readonly Dictionary<string, TypedTag> _tags;

    internal ExifDirectory(ImmutableArray<TypedTag> tags)
    {
        _tags = new(StringComparer.Ordinal);
        foreach (var t in tags)
            if (t.Group is "EXIF" && !_tags.ContainsKey(t.Name))
                _tags[t.Name] = t;
    }

    private TypedTag? Get(string name) => _tags.TryGetValue(name, out var t) ? t : null;
    private string? Str(string name) => Get(name)?.Value;

    // -----------------------------------------------------------------------
    // IFD0 - TIFF baseline / primary image
    // -----------------------------------------------------------------------

    public string? SubfileType => Str("SubfileType");
    public string? OldSubfileType => Str("OldSubfileType");
    public int? ImageWidth => Get("ImageWidth")?.AsInt32() ?? Get("ExifImageWidth")?.AsInt32();
    public int? ImageHeight => Get("ImageHeight")?.AsInt32() ?? Get("ExifImageHeight")?.AsInt32();
    public int? BitsPerSample => Get("BitsPerSample")?.AsInt32();
    public string? Compression => Str("Compression");
    public string? PhotometricInterpretation => Str("PhotometricInterpretation");
    public string? FillOrder => Str("FillOrder");
    public string? DocumentName => Str("DocumentName");
    public string? ImageDescription => Str("ImageDescription");
    public string? Make => Str("Make");
    public string? Model => Str("Model");
    public int? StripOffsets => Get("StripOffsets")?.AsInt32();
    public ExifOrientation? Orientation => Get("Orientation")?.AsInt32() is int v
        && Enum.IsDefined((ExifOrientation)v) ? (ExifOrientation)v : null;
    public int? SamplesPerPixel => Get("SamplesPerPixel")?.AsInt32();
    public int? RowsPerStrip => Get("RowsPerStrip")?.AsInt32();
    public int? StripByteCounts => Get("StripByteCounts")?.AsInt32();
    public int? MinSampleValue => Get("MinSampleValue")?.AsInt32();
    public int? MaxSampleValue => Get("MaxSampleValue")?.AsInt32();
    public double? XResolution => Get("XResolution")?.AsDouble();
    public double? YResolution => Get("YResolution")?.AsDouble();
    public string? PlanarConfiguration => Str("PlanarConfiguration");
    public double? XPosition => Get("XPosition")?.AsDouble();
    public double? YPosition => Get("YPosition")?.AsDouble();
    public string? ResolutionUnit => Str("ResolutionUnit");
    public string? Software => Str("Software");
    public string? DateTime => Str("DateTime");
    public string? Artist => Str("Artist");
    public string? HostComputer => Str("HostComputer");
    public string? Predictor => Str("Predictor");
    public string? WhitePoint => Str("WhitePoint");
    public string? PrimaryChromaticities => Str("PrimaryChromaticities");
    public string? PageName => Str("PageName");
    public string? PageNumber => Str("PageNumber");
    public string? ColorMap => Str("ColorMap");
    public int? TileWidth => Get("TileWidth")?.AsInt32();
    public int? TileLength => Get("TileLength")?.AsInt32();
    public string? TileOffsets => Str("TileOffsets");
    public string? TileByteCounts => Str("TileByteCounts");
    public string? SubIFDs => Str("SubIFDs");
    public string? JPEGTables => Str("JPEGTables");
    public string? ExtraSamples => Str("ExtraSamples");
    public string? SampleFormat => Str("SampleFormat");
    public string? YCbCrCoefficients => Str("YCbCrCoefficients");
    public string? YCbCrSubSampling => Str("YCbCrSubSampling");
    public string? YCbCrPositioning => Str("YCbCrPositioning");
    public string? ReferenceBlackWhite => Str("ReferenceBlackWhite");
    public string? Gamma => Str("Gamma");
    public string? Matteing => Str("Matteing");
    public string? ModelTransform => Str("ModelTransform");
    public string? TiffEpStandardId => Str("TIFF-EPStandardID");
    public string? Copyright => Str("Copyright");
    public string? XPTitle => Str("XPTitle");
    public string? XPComment => Str("XPComment");
    public string? XPAuthor => Str("XPAuthor");
    public string? XPKeywords => Str("XPKeywords");
    public string? XPSubject => Str("XPSubject");

    // -----------------------------------------------------------------------
    // ExifIFD - camera / exposure settings
    // -----------------------------------------------------------------------

    public (int Num, int Den)? ExposureTime => Get("ExposureTime")?.AsRational();
    public double? FNumber => Get("FNumber")?.AsDouble();
    public string? ExposureProgram => Str("ExposureProgram");
    public string? SpectralSensitivity => Str("SpectralSensitivity");
    public int? Iso => Get("ISO")?.AsInt32();
    public string? Oecf => Str("OECF");
    public string? SensitivityType => Str("SensitivityType");
    public int? StandardOutputSensitivity => Get("StandardOutputSensitivity")?.AsInt32();
    public int? RecommendedExposureIndex => Get("RecommendedExposureIndex")?.AsInt32();
    public int? IsoSpeed => Get("ISOSpeed")?.AsInt32();
    public string? ExifVersion => Str("ExifVersion");
    public string? DateTimeOriginal => Str("DateTimeOriginal");
    public string? DateTimeDigitized => Str("DateTimeDigitized");
    public string? OffsetTime => Str("OffsetTime");
    public string? OffsetTimeOriginal => Str("OffsetTimeOriginal");
    public string? OffsetTimeDigitized => Str("OffsetTimeDigitized");
    public string? ComponentsConfiguration => Str("ComponentsConfiguration");
    public double? CompressedBitsPerPixel => Get("CompressedBitsPerPixel")?.AsDouble();
    public double? ShutterSpeedValue => Get("ShutterSpeedValue")?.AsDouble();
    public double? ApertureValue => Get("ApertureValue")?.AsDouble();
    public double? BrightnessValue => Get("BrightnessValue")?.AsDouble();
    public double? ExposureCompensation => Get("ExposureCompensation")?.AsDouble();
    public double? MaxApertureValue => Get("MaxApertureValue")?.AsDouble();
    public double? SubjectDistance => Get("SubjectDistance")?.AsDouble();
    public string? MeteringMode => Str("MeteringMode");
    public string? LightSource => Str("LightSource");
    public string? Flash => Str("Flash");
    public double? FocalLength => Get("FocalLength")?.AsDouble();
    public string? SubjectArea => Str("SubjectArea");
    public string? UserComment => Str("UserComment");
    public string? SubSecTime => Str("SubSecTime");
    public string? SubSecTimeOriginal => Str("SubSecTimeOriginal");
    public string? SubSecTimeDigitized => Str("SubSecTimeDigitized");
    public string? FlashpixVersion => Str("FlashpixVersion");
    public string? ColorSpace => Str("ColorSpace");
    public int? ExifImageWidth => Get("ExifImageWidth")?.AsInt32();
    public int? ExifImageHeight => Get("ExifImageHeight")?.AsInt32();
    public string? RelatedSoundFile => Str("RelatedSoundFile");
    public double? FlashEnergy => Get("FlashEnergy")?.AsDouble();
    public double? FocalPlaneXResolution => Get("FocalPlaneXResolution")?.AsDouble();
    public double? FocalPlaneYResolution => Get("FocalPlaneYResolution")?.AsDouble();
    public string? FocalPlaneResolutionUnit => Str("FocalPlaneResolutionUnit");
    public double? ExposureIndex => Get("ExposureIndex")?.AsDouble();
    public string? SensingMethod => Str("SensingMethod");
    public string? FileSource => Str("FileSource");
    public string? SceneType => Str("SceneType");
    public string? CfaPattern => Str("CFAPattern");
    public string? CustomRendered => Str("CustomRendered");
    public string? ExposureMode => Str("ExposureMode");
    public string? WhiteBalance => Str("WhiteBalance");
    public double? DigitalZoomRatio => Get("DigitalZoomRatio")?.AsDouble();
    public int? FocalLengthIn35mmFormat => Get("FocalLengthIn35mmFormat")?.AsInt32();
    public string? SceneCaptureType => Str("SceneCaptureType");
    public string? GainControl => Str("GainControl");
    public string? Contrast => Str("Contrast");
    public string? Saturation => Str("Saturation");
    public string? Sharpness => Str("Sharpness");
    public string? DeviceSettingDescription => Str("DeviceSettingDescription");
    public string? SubjectDistanceRange => Str("SubjectDistanceRange");
    public string? ImageUniqueId => Str("ImageUniqueID");
    public string? CameraOwnerName => Str("CameraOwnerName");
    public string? SerialNumber => Str("SerialNumber");
    public string? LensInfo => Str("LensInfo");
    public string? LensMake => Str("LensMake");
    public string? LensModel => Str("LensModel");
    public string? LensSerialNumber => Str("LensSerialNumber");
    public string? GammaExif => Str("Gamma");
    public string? CompositeImage => Str("CompositeImage");
    public string? CompositeImageCount => Str("CompositeImageCount");
    public string? CompositeImageExposureTimes => Str("CompositeImageExposureTimes");
    public string? Padding => Str("Padding");
    public string? OffsetSchema => Str("OffsetSchema");

    // -----------------------------------------------------------------------
    // Interop IFD
    // -----------------------------------------------------------------------

    public string? InteropIndex => Str("InteropIndex");
    public string? InteropVersion => Str("InteropVersion");
    public string? RelatedImageFileFormat => Str("RelatedImageFileFormat");
    public int? RelatedImageWidth => Get("RelatedImageWidth")?.AsInt32();
    public int? RelatedImageHeight => Get("RelatedImageHeight")?.AsInt32();

    /// <summary>Look up any EXIF tag by name.</summary>
    public TypedTag? this[string name] => Get(name);
}
