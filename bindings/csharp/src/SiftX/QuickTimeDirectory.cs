using System.Collections.Immutable;

namespace SiftX;

/// <summary>
/// Strongly-typed access to QuickTime/MP4 container metadata.
/// </summary>
public sealed class QuickTimeDirectory
{
    private readonly Dictionary<string, TypedTag> _tags;

    internal QuickTimeDirectory(ImmutableArray<TypedTag> tags)
    {
        _tags = new(StringComparer.Ordinal);
        foreach (var t in tags)
            if (t.Group is "QuickTime" && !_tags.ContainsKey(t.Name))
                _tags[t.Name] = t;
    }

    private string? Str(string name) => _tags.TryGetValue(name, out var t) ? t.Value : null;

    // Container
    public string? MajorBrand => Str("MajorBrand");
    public string? MinorVersion => Str("MinorVersion");
    public string? CompatibleBrands => Str("CompatibleBrands");

    // Timing
    public string? CreateDate => Str("CreateDate");
    public string? ModifyDate => Str("ModifyDate");
    public string? TimeScale => Str("TimeScale");
    public string? Duration => Str("Duration");

    // Video
    public string? ImageWidth => Str("ImageWidth");
    public string? ImageHeight => Str("ImageHeight");
    public string? CompressorId => Str("CompressorID");
    public string? CompressorName => Str("CompressorName");
    public string? VideoFrameRate => Str("VideoFrameRate");
    public string? HandlerDescription => Str("HandlerDescription");

    // Audio
    public string? AudioFormat => Str("AudioFormat");
    public string? AudioChannels => Str("AudioChannels");
    public string? AudioBitsPerSample => Str("AudioBitsPerSample");
    public string? AudioSampleRate => Str("AudioSampleRate");

    // GPS
    public string? GpsCoordinates => Str("GPSCoordinates");

    /// <summary>Look up any QuickTime tag by name.</summary>
    public TypedTag? this[string name] => _tags.TryGetValue(name, out var t) ? t : null;
}
