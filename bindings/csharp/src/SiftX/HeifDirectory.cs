using System.Collections.Immutable;

namespace SiftX;

/// <summary>
/// Strongly-typed access to HEIF/HEIC container metadata.
/// </summary>
public sealed class HeifDirectory
{
    private readonly Dictionary<string, TypedTag> _tags;

    internal HeifDirectory(ImmutableArray<TypedTag> tags)
    {
        _tags = new(StringComparer.Ordinal);
        foreach (var t in tags)
            if (t.Group is "HEIF" && !_tags.ContainsKey(t.Name))
                _tags[t.Name] = t;
    }

    private string? Str(string name) => _tags.TryGetValue(name, out var t) ? t.Value : null;

    // Container
    public string? MajorBrand => Str("MajorBrand");
    public string? MinorVersion => Str("MinorVersion");
    public string? CompatibleBrands => Str("CompatibleBrands");
    public string? HandlerType => Str("HandlerType");
    public string? PrimaryItemReference => Str("PrimaryItemReference");

    // Image
    public string? Rotation => Str("Rotation");
    public string? ImagePixelDepth => Str("ImagePixelDepth");
    public string? ImageSpatialExtent => Str("ImageSpatialExtent");
    public string? MetaImageSize => Str("MetaImageSize");
    public string? MediaDataOffset => Str("MediaDataOffset");
    public string? MediaDataSize => Str("MediaDataSize");
    public string? CompressorId => Str("CompressorID");
    public string? AuxiliaryImageType => Str("AuxiliaryImageType");

    // HEVC Configuration
    public string? HevcConfigurationVersion => Str("HEVCConfigurationVersion");
    public string? GeneralProfileSpace => Str("GeneralProfileSpace");
    public string? GeneralTierFlag => Str("GeneralTierFlag");
    public string? GeneralProfileIdc => Str("GeneralProfileIDC");
    public string? GeneralLevelIdc => Str("GeneralLevelIDC");
    public string? GenProfileCompatibilityFlags => Str("GenProfileCompatibilityFlags");
    public string? ConstraintIndicatorFlags => Str("ConstraintIndicatorFlags");
    public string? ChromaFormat => Str("ChromaFormat");
    public string? BitDepthLuma => Str("BitDepthLuma");
    public string? BitDepthChroma => Str("BitDepthChroma");
    public string? MinSpatialSegmentationIdc => Str("MinSpatialSegmentationIDC");
    public string? ParallelismType => Str("ParallelismType");
    public string? NumTemporalLayers => Str("NumTemporalLayers");
    public string? TemporalIdNested => Str("TemporalIdNested");
    public string? ConstantFrameRate => Str("ConstantFrameRate");
    public string? AverageFrameRate => Str("AverageFrameRate");

    /// <summary>Look up any HEIF tag by name.</summary>
    public TypedTag? this[string name] => _tags.TryGetValue(name, out var t) ? t : null;
}
