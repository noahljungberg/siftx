using System.Collections.Immutable;

namespace SiftX;

/// <summary>
/// Strongly-typed access to ICC color profile metadata.
/// </summary>
public sealed class IccDirectory
{
    private readonly Dictionary<string, TypedTag> _tags;

    internal IccDirectory(ImmutableArray<TypedTag> tags)
    {
        _tags = new(StringComparer.Ordinal);
        foreach (var t in tags)
            if (t.Group is "ICC" && !_tags.ContainsKey(t.Name))
                _tags[t.Name] = t;
    }

    private string? Str(string name) => _tags.TryGetValue(name, out var t) ? t.Value : null;

    public string? ProfileDescription => Str("ProfileDescription");
    public string? ProfileCopyright => Str("ProfileCopyright");
    public string? ProfileCmmType => Str("ProfileCMMType");
    public string? ProfileVersion => Str("ProfileVersion");
    public string? ProfileClass => Str("ProfileClass");
    public string? ColorSpaceData => Str("ColorSpaceData");
    public string? ProfileConnectionSpace => Str("ProfileConnectionSpace");
    public string? ProfileDateTime => Str("ProfileDateTime");
    public string? ProfileFileSignature => Str("ProfileFileSignature");
    public string? PrimaryPlatform => Str("PrimaryPlatform");
    public string? CmmFlags => Str("CMMFlags");
    public string? DeviceManufacturer => Str("DeviceManufacturer");
    public string? DeviceModel => Str("DeviceModel");
    public string? DeviceAttributes => Str("DeviceAttributes");
    public string? RenderingIntent => Str("RenderingIntent");
    public string? ConnectionSpaceIlluminant => Str("ConnectionSpaceIlluminant");
    public string? ProfileCreator => Str("ProfileCreator");
    public string? ProfileId => Str("ProfileID");
    public string? MediaWhitePoint => Str("MediaWhitePoint");
    public string? ChromaticAdaptation => Str("ChromaticAdaptation");
    public string? RedMatrixColumn => Str("RedMatrixColumn");
    public string? GreenMatrixColumn => Str("GreenMatrixColumn");
    public string? BlueMatrixColumn => Str("BlueMatrixColumn");
    public string? RedToneReproductionCurve => Str("RedToneReproductionCurve");
    public string? GreenToneReproductionCurve => Str("GreenToneReproductionCurve");
    public string? BlueToneReproductionCurve => Str("BlueToneReproductionCurve");

    /// <summary>Look up any ICC tag by name.</summary>
    public TypedTag? this[string name] => _tags.TryGetValue(name, out var t) ? t : null;
}
