using System.Collections.Immutable;

namespace SiftX;

/// <summary>
/// Access to vendor-specific MakerNotes metadata.
/// MakerNotes are camera-vendor specific - the same tag name can appear
/// across different vendors with different meanings. Use the <c>this[name]</c>
/// indexer for arbitrary tag access, or the common cross-vendor properties below.
/// </summary>
public sealed class MakerNotesDirectory
{
    private readonly Dictionary<string, TypedTag> _tags;

    internal MakerNotesDirectory(ImmutableArray<TypedTag> tags)
    {
        _tags = new(StringComparer.Ordinal);
        foreach (var t in tags)
            if (t.Group is "MakerNotes" && !_tags.ContainsKey(t.Name))
                _tags[t.Name] = t;
    }

    private string? Str(string name) => _tags.TryGetValue(name, out var t) ? t.Value : null;

    /// <summary>Number of MakerNotes tags present.</summary>
    public int Count => _tags.Count;

    // -----------------------------------------------------------------------
    // Common cross-vendor tags (appear in most camera brands)
    // -----------------------------------------------------------------------

    public string? MakerNoteVersion => Str("MakerNoteVersion");
    public string? SerialNumber => Str("SerialNumber");
    public string? InternalSerialNumber => Str("InternalSerialNumber");
    public string? FirmwareVersion => Str("FirmwareVersion");
    public string? Firmware => Str("Firmware");
    public string? LensModel => Str("LensModel");
    public string? LensType => Str("LensType");
    public string? LensInfo => Str("LensInfo");
    public string? LensSerialNumber => Str("LensSerialNumber");
    public string? LensFirmwareVersion => Str("LensFirmwareVersion");

    // Focus
    public string? AFMode => Str("AFMode");
    public string? AFPoint => Str("AFPoint");
    public string? AFPointsInFocus => Str("AFPointsInFocus");
    public string? AFAreaMode => Str("AFAreaMode");
    public string? FocusMode => Str("FocusMode");
    public string? FocusDistance => Str("FocusDistance");
    public string? FocusPosition => Str("FocusPosition");

    // Exposure
    public string? ISO => Str("ISO");
    public string? ExposureTime => Str("ExposureTime");
    public string? FNumber => Str("FNumber");
    public string? ExposureCompensation => Str("ExposureCompensation");
    public string? ExposureMode => Str("ExposureMode");
    public string? MeteringMode => Str("MeteringMode");

    // Flash
    public string? FlashMode => Str("FlashMode");
    public string? FlashType => Str("FlashType");
    public string? FlashSetting => Str("FlashSetting");
    public string? FlashExposureComp => Str("FlashExposureComp");
    public string? FlashFired => Str("FlashFired");

    // White balance
    public string? WhiteBalance => Str("WhiteBalance");
    public string? WhiteBalanceTemperature => Str("WhiteBalanceTemperature");
    public string? ColorTemperature => Str("ColorTemperature");

    // Image processing
    public string? Quality => Str("Quality");
    public string? ImageQuality => Str("ImageQuality");
    public string? ColorMode => Str("ColorMode");
    public string? ColorSpace => Str("ColorSpace");
    public string? Contrast => Str("Contrast");
    public string? Saturation => Str("Saturation");
    public string? Sharpness => Str("Sharpness");
    public string? NoiseReduction => Str("NoiseReduction");
    public string? HighISONoiseReduction => Str("HighISONoiseReduction");
    public string? ImageStabilization => Str("ImageStabilization");

    // Scene / mode
    public string? ShootingMode => Str("ShootingMode");
    public string? DriveMode => Str("DriveMode");
    public string? SceneMode => Str("SceneMode");
    public string? MacroMode => Str("MacroMode");
    public string? PictureMode => Str("PictureMode");
    public string? RecordMode => Str("RecordMode");

    // Shutter / mechanical
    public string? ShutterCount => Str("ShutterCount");
    public string? ShutterMode => Str("ShutterMode");
    public string? MechanicalShutterCount => Str("MechanicalShutterCount");

    // Camera info
    public string? CameraType => Str("CameraType");
    public string? CameraOrientation => Str("CameraOrientation");
    public string? CameraTemperature => Str("CameraTemperature");
    public string? BatteryInfo => Str("BatteryInfo");
    public string? BatteryType => Str("BatteryType");
    public string? Software => Str("Software");
    public string? ImageWidth => Str("ImageWidth");
    public string? ImageHeight => Str("ImageHeight");

    // Dynamic range
    public string? DynamicRange => Str("DynamicRange");
    public string? DynamicRangeOptimizer => Str("DynamicRangeOptimizer");
    public string? HDR => Str("HDR");

    // Vendor-specific model IDs
    public string? CanonModelId => Str("CanonModelID");
    public string? SonyModelId => Str("SonyModelID");
    public string? PentaxModelId => Str("PentaxModelID");

    /// <summary>Look up any MakerNotes tag by name.</summary>
    public TypedTag? this[string name] => _tags.TryGetValue(name, out var t) ? t : null;

    /// <summary>All MakerNotes tags as an immutable array.</summary>
    public ImmutableArray<TypedTag> All => [.. _tags.Values];
}
