using System.Collections.Immutable;

namespace SiftX;

/// <summary>
/// Typed access to all GPS EXIF properties.
/// For computed decimal coordinates, use <see cref="Coordinates"/>.
/// </summary>
public sealed class GpsDirectory
{
    private readonly Dictionary<string, TypedTag> _tags;

    internal GpsDirectory(ImmutableArray<TypedTag> tags, GpsCoordinates? coordinates)
    {
        _tags = new(StringComparer.Ordinal);
        foreach (var t in tags)
            if (t.Group is "EXIF" && t.Name.StartsWith("GPS", StringComparison.Ordinal)
                && !_tags.ContainsKey(t.Name))
                _tags[t.Name] = t;
        Coordinates = coordinates;
    }

    private TypedTag? Get(string name) => _tags.TryGetValue(name, out var t) ? t : null;
    private string? Str(string name) => Get(name)?.Value;

    // All 32 GPS tags
    public string? VersionId => Str("GPSVersionID");
    public string? LatitudeRef => Str("GPSLatitudeRef");
    public string? Latitude => Str("GPSLatitude");
    public string? LongitudeRef => Str("GPSLongitudeRef");
    public string? Longitude => Str("GPSLongitude");
    public string? AltitudeRef => Str("GPSAltitudeRef");
    public string? Altitude => Str("GPSAltitude");
    public string? TimeStamp => Str("GPSTimeStamp");
    public string? Satellites => Str("GPSSatellites");
    public string? Status => Str("GPSStatus");
    public string? MeasureMode => Str("GPSMeasureMode");
    public double? Dop => Get("GPSDOP")?.AsDouble();
    public string? SpeedRef => Str("GPSSpeedRef");
    public double? Speed => Get("GPSSpeed")?.AsDouble();
    public string? TrackRef => Str("GPSTrackRef");
    public double? Track => Get("GPSTrack")?.AsDouble();
    public string? ImgDirectionRef => Str("GPSImgDirectionRef");
    public double? ImgDirection => Get("GPSImgDirection")?.AsDouble();
    public string? MapDatum => Str("GPSMapDatum");
    public string? DestLatitudeRef => Str("GPSDestLatitudeRef");
    public string? DestLatitude => Str("GPSDestLatitude");
    public string? DestLongitudeRef => Str("GPSDestLongitudeRef");
    public string? DestLongitude => Str("GPSDestLongitude");
    public string? DestBearingRef => Str("GPSDestBearingRef");
    public double? DestBearing => Get("GPSDestBearing")?.AsDouble();
    public string? DestDistanceRef => Str("GPSDestDistanceRef");
    public double? DestDistance => Get("GPSDestDistance")?.AsDouble();
    public string? ProcessingMethod => Str("GPSProcessingMethod");
    public string? AreaInformation => Str("GPSAreaInformation");
    public string? DateStamp => Str("GPSDateStamp");
    public string? Differential => Str("GPSDifferential");
    public double? HPositioningError => Get("GPSHPositioningError")?.AsDouble();

    /// <summary>Computed decimal-degree coordinates. Null if no GPS data.</summary>
    public GpsCoordinates? Coordinates { get; }

    /// <summary>Look up any GPS tag by name.</summary>
    public TypedTag? this[string name] => Get(name);
}
