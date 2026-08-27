namespace SiftX.Benchmarks;

internal static class TestPaths
{
    private static readonly string RepoRoot = FindRepoRoot();

    public static string ExifToolImages => Path.Combine(RepoRoot, "testdata", "exiftool-images");
    public static string ExifSamples => Path.Combine(RepoRoot, "testdata", "exif-samples");
    public static string PopplerTest => Path.Combine(RepoRoot, "testdata", "poppler-test");

    public static bool HasExifToolImages => Directory.Exists(ExifToolImages);
    public static bool HasExifSamples => Directory.Exists(ExifSamples);
    public static bool HasPopplerTest => Directory.Exists(PopplerTest);

    private static string FindRepoRoot()
    {
        var dir = Directory.GetCurrentDirectory();
        while (dir != null)
        {
            if (File.Exists(Path.Combine(dir, "Cargo.toml")))
                return dir;
            dir = Directory.GetParent(dir)?.FullName;
        }
        return Path.GetFullPath(Path.Combine(AppContext.BaseDirectory, "..", "..", "..", "..", "..", "..", ".."));
    }
}
