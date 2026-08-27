using System.Collections.Immutable;

namespace SiftX;

/// <summary>
/// Strongly-typed access to all IPTC Application Record (IIM) metadata.
/// Every standard IPTC dataset is exposed as a property.
/// </summary>
public sealed class IptcDirectory
{
    private readonly Dictionary<string, TypedTag> _tags;

    internal IptcDirectory(ImmutableArray<TypedTag> tags)
    {
        _tags = new(StringComparer.Ordinal);
        foreach (var t in tags)
            if (t.Group is "IPTC" && !_tags.ContainsKey(t.Name))
                _tags[t.Name] = t;
    }

    private string? Str(string name) => _tags.TryGetValue(name, out var t) ? t.Value : null;

    // Record 1
    public string? CodedCharacterSet => Str("CodedCharacterSet");

    // Record 2 - Application Record (all 55 datasets)
    public string? ApplicationRecordVersion => Str("ApplicationRecordVersion");
    public string? ObjectTypeReference => Str("ObjectTypeReference");
    public string? ObjectAttributeReference => Str("ObjectAttributeReference");
    public string? ObjectName => Str("ObjectName");
    public string? EditStatus => Str("EditStatus");
    public string? EditorialUpdate => Str("EditorialUpdate");
    public string? Urgency => Str("Urgency");
    public string? SubjectReference => Str("SubjectReference");
    public string? Category => Str("Category");
    public string? SupplementalCategories => Str("SupplementalCategories");
    public string? FixtureIdentifier => Str("FixtureIdentifier");
    public string? Keywords => Str("Keywords");
    public string? ContentLocationCode => Str("ContentLocationCode");
    public string? ContentLocationName => Str("ContentLocationName");
    public string? ReleaseDate => Str("ReleaseDate");
    public string? ReleaseTime => Str("ReleaseTime");
    public string? ExpirationDate => Str("ExpirationDate");
    public string? ExpirationTime => Str("ExpirationTime");
    public string? SpecialInstructions => Str("SpecialInstructions");
    public string? ActionAdvised => Str("ActionAdvised");
    public string? ReferenceService => Str("ReferenceService");
    public string? ReferenceDate => Str("ReferenceDate");
    public string? ReferenceNumber => Str("ReferenceNumber");
    public string? DateCreated => Str("DateCreated");
    public string? TimeCreated => Str("TimeCreated");
    public string? DigitalCreationDate => Str("DigitalCreationDate");
    public string? DigitalCreationTime => Str("DigitalCreationTime");
    public string? OriginatingProgram => Str("OriginatingProgram");
    public string? ProgramVersion => Str("ProgramVersion");
    public string? ObjectCycle => Str("ObjectCycle");
    public string? Byline => Str("By-line");
    public string? BylineTitle => Str("By-lineTitle");
    public string? City => Str("City");
    public string? SubLocation => Str("Sub-location");
    public string? ProvinceState => Str("Province-State");
    public string? CountryCode => Str("Country-PrimaryLocationCode");
    public string? Country => Str("Country-PrimaryLocationName");
    public string? OriginalTransmissionReference => Str("OriginalTransmissionReference");
    public string? Headline => Str("Headline");
    public string? Credit => Str("Credit");
    public string? Source => Str("Source");
    public string? Copyright => Str("CopyrightNotice");
    public string? Contact => Str("Contact");
    public string? Caption => Str("Caption-Abstract");
    public string? WriterEditor => Str("Writer-Editor");
    public string? RasterizedCaption => Str("RasterizedCaption");
    public string? ImageType => Str("ImageType");
    public string? ImageOrientation => Str("ImageOrientation");
    public string? LanguageIdentifier => Str("LanguageIdentifier");
    public string? AudioType => Str("AudioType");
    public string? AudioSamplingRate => Str("AudioSamplingRate");
    public string? AudioSamplingResolution => Str("AudioSamplingResolution");
    public string? AudioDuration => Str("AudioDuration");
    public string? AudioOutcue => Str("AudioOutcue");
    public string? ObjectPreviewFileFormat => Str("ObjectPreviewFileFormat");
    public string? ObjectPreviewFileVersion => Str("ObjectPreviewFileVersion");
    public string? ObjectPreviewData => Str("ObjectPreviewData");

    /// <summary>Look up any IPTC tag by name.</summary>
    public TypedTag? this[string name] => _tags.TryGetValue(name, out var t) ? t : null;
}
