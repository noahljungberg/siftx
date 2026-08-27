namespace SiftX.Tests;

public class MultiFormatTests
{
    [SkippableFact]
    public void ScanAllFormats_NoExceptions()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var extensions = new[] { "*.jpg", "*.png", "*.gif", "*.bmp", "*.tif", "*.tiff", "*.webp" };
        int total = 0;
        int parsed = 0;

        foreach (var ext in extensions)
        {
            foreach (var path in Directory.GetFiles(TestPaths.ExifToolImages, ext).Take(5))
            {
                total++;
                try
                {
                    using var file = SiftFile.Open(path);
                    using var doc = file.Parse();
                    _ = doc.Tags();
                    parsed++;
                }
                catch (SiftException)
                {
                    // Acceptable - some test files may be intentionally malformed
                }
            }
        }

        Assert.True(total > 0, "should find test files");
        Assert.True(parsed > 0, "should parse at least some files");
    }

    [SkippableFact]
    public void ReadFromBuffer_AllFormats()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var files = new[]
        {
            ("*.jpg", FileType.Jpeg),
            ("*.png", FileType.Png),
            ("*.gif", FileType.Gif),
        };

        foreach (var (pattern, expectedType) in files)
        {
            var path = Directory.GetFiles(TestPaths.ExifToolImages, pattern).FirstOrDefault();
            if (path is null) continue;

            var data = File.ReadAllBytes(path);
            using var doc = SiftLib.Read(data);
            Assert.Equal(expectedType, doc.FileType);
        }
    }

    [SkippableFact]
    public void Tags_ImmutableArray_IsThreadSafe()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var data = File.ReadAllBytes(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = SiftLib.Read(data);
        var tags = doc.Tags();

        // ImmutableArray can be safely shared across threads
        var tasks = Enumerable.Range(0, 4).Select(_ => Task.Run(() =>
        {
            foreach (var tag in tags)
            {
                var s = tag.ToString();
                Assert.NotNull(s);
            }
            return tags.Length;
        })).ToArray();

        var results = Task.WhenAll(tasks).Result;
        Assert.All(results, r => Assert.Equal(tags.Length, r));
    }
}
