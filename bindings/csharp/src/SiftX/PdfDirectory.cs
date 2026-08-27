using System.Collections.Immutable;

namespace SiftX;

/// <summary>
/// Strongly-typed access to PDF document metadata.
/// </summary>
public sealed class PdfDirectory
{
    private readonly Dictionary<string, TypedTag> _tags;

    internal PdfDirectory(ImmutableArray<TypedTag> tags)
    {
        _tags = new(StringComparer.Ordinal);
        foreach (var t in tags)
            if (t.Group is "PDF" && !_tags.ContainsKey(t.Name))
                _tags[t.Name] = t;
    }

    private string? Str(string name) => _tags.TryGetValue(name, out var t) ? t.Value : null;

    public string? Title => Str("Title");
    public string? Author => Str("Author");
    public string? Subject => Str("Subject");
    public string? Keywords => Str("Keywords");
    public string? Creator => Str("Creator");
    public string? Producer => Str("Producer");
    public string? CreationDate => Str("CreationDate");
    public string? ModDate => Str("ModDate");
    public string? PdfVersion => Str("PDFVersion");
    public int? PageCount => Str("PageCount") is string s && int.TryParse(s, out var n) ? n : null;
    public string? PageSize => Str("PageSize");
    public bool IsTagged => Str("Tagged") is "Yes";
    public bool IsEncrypted => Str("Encrypted") is "Yes";
    public bool IsLinearized => Str("Linearized") is "Yes";
    public bool HasJavaScript => Str("JavaScript") is "Yes";

    /// <summary>Look up any PDF tag by name.</summary>
    public TypedTag? this[string name] => _tags.TryGetValue(name, out var t) ? t : null;
}
