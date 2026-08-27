namespace SiftX.Tests;

internal static class TestPaths
{
    // Relative to the test execution directory - climb up to repo root
    private static readonly string RepoRoot = FindRepoRoot();

    public static string ExifToolImages => Path.Combine(RepoRoot, "testdata", "exiftool-images");
    public static string ExifSamples => Path.Combine(RepoRoot, "testdata", "exif-samples");
    public static string PopplerTest => Path.Combine(RepoRoot, "testdata", "poppler-test");
    public static string PdfjsPdfs => Path.Combine(RepoRoot, "testdata", "pdfjs-pdfs");

    public static bool HasExifToolImages => Directory.Exists(ExifToolImages);
    public static bool HasExifSamples => Directory.Exists(ExifSamples);
    public static bool HasPopplerTest => Directory.Exists(PopplerTest);
    public static bool HasPdfjsPdfs => Directory.Exists(PdfjsPdfs);

    private static string FindRepoRoot()
    {
        // Walk up from current directory to find repo root (has Cargo.toml)
        var dir = Directory.GetCurrentDirectory();
        while (dir != null)
        {
            if (File.Exists(Path.Combine(dir, "Cargo.toml")))
                return dir;
            dir = Directory.GetParent(dir)?.FullName;
        }
        // Fallback: try relative from bin output
        return Path.GetFullPath(Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..", "..", ".."));
    }
}
