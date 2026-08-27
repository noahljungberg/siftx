using System.Collections.Immutable;

namespace SiftX;

/// <summary>
/// Strongly-typed access to computed/composite metadata tags.
/// These are derived values calculated from raw EXIF data (e.g., 35mm equivalent
/// focal length, combined date+timezone, hyperfocal distance).
/// </summary>
public sealed class CompositeDirectory
{
    private readonly Dictionary<string, TypedTag> _tags;

    internal CompositeDirectory(ImmutableArray<TypedTag> tags)
    {
        _tags = new(StringComparer.Ordinal);
        foreach (var t in tags)
            if (t.Group is "Composite" && !_tags.ContainsKey(t.Name))
                _tags[t.Name] = t;
    }

    private string? Str(string name) => _tags.TryGetValue(name, out var t) ? t.Value : null;

    // Image dimensions
    public string? ImageWidth => Str("ImageWidth");
    public string? ImageHeight => Str("ImageHeight");
    public string? ImageSize => Str("ImageSize");
    public string? Megapixels => Str("Megapixels");

    // Exposure (computed display values)
    public string? Aperture => Str("Aperture");
    public string? ShutterSpeed => Str("ShutterSpeed");
    public string? LightValue => Str("LightValue");

    // Focal length
    public string? ScaleFactor35efl => Str("ScaleFactor35efl");
    public string? ScaleFactorTo35mmEquivalent => Str("ScaleFactorTo35mmEquivalent");
    public string? FocalLength35efl => Str("FocalLength35efl");
    public string? FieldOfView => Str("FieldOfView");
    public string? Fov => Str("FOV");

    // Depth of field
    public string? CircleOfConfusion => Str("CircleOfConfusion");
    public string? HyperfocalDistance => Str("HyperfocalDistance");

    // GPS
    public string? GpsPosition => Str("GPSPosition");

    // Camera
    public string? CameraModelName => Str("CameraModelName");
    public string? LensId => Str("LensID");

    // Date/time (combined with subseconds and timezone)
    public string? DateTimeOriginal => Str("DateTimeOriginal");
    public string? CreateDate => Str("CreateDate");
    public string? ModifyDate => Str("ModifyDate");
    public string? SubSecDateTimeOriginal => Str("SubSecDateTimeOriginal");
    public string? SubSecCreateDate => Str("SubSecCreateDate");
    public string? SubSecModifyDate => Str("SubSecModifyDate");

    // Runtime
    public string? RunTimeSincePowerUp => Str("RunTimeSincePowerUp");

    /// <summary>Look up any Composite tag by name.</summary>
    public TypedTag? this[string name] => _tags.TryGetValue(name, out var t) ? t : null;
}
