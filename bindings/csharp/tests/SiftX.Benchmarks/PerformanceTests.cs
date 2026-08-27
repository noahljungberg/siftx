using System.Diagnostics;
using MetadataExtractor;
using Xunit.Abstractions;

namespace SiftX.Benchmarks;

/// <summary>
/// Head-to-head performance comparison: Sift (native Rust) vs MetadataExtractor (pure C#).
/// Measures wall-clock time for extracting metadata from real-world test images.
/// </summary>
public class PerformanceTests(ITestOutputHelper output)
{
    // -----------------------------------------------------------------------
    // JPEG batch - ExifTool test images (41 files)
    // -----------------------------------------------------------------------

    [SkippableFact]
    public void Perf_Jpeg_ExifToolImages_Sift()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");
        var files = System.IO.Directory.GetFiles(TestPaths.ExifToolImages, "*.jpg");
        Skip.If(files.Length == 0, "no JPEG files");

        // Warmup
        using (var f = SiftFile.Open(files[0])) { using var d = f.Parse(); _ = d.Tags(); }

        var sw = Stopwatch.StartNew();
        int totalTags = 0;
        foreach (var path in files)
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                totalTags += doc.Tags().Length;
            }
            catch (SiftException) { }
        }
        sw.Stop();

        output.WriteLine($"Sift: {files.Length} JPEGs, {totalTags} tags, {sw.ElapsedMilliseconds}ms ({sw.ElapsedMilliseconds * 1000.0 / files.Length:F0}µs/file)");
    }

    [SkippableFact]
    public void Perf_Jpeg_ExifToolImages_MetadataExtractor()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");
        var files = System.IO.Directory.GetFiles(TestPaths.ExifToolImages, "*.jpg");
        Skip.If(files.Length == 0, "no JPEG files");

        // Warmup
        _ = ImageMetadataReader.ReadMetadata(files[0]);

        var sw = Stopwatch.StartNew();
        int totalTags = 0;
        foreach (var path in files)
        {
            try
            {
                var dirs = ImageMetadataReader.ReadMetadata(path);
                totalTags += dirs.Sum(d => d.TagCount);
            }
            catch { }
        }
        sw.Stop();

        output.WriteLine($"MetadataExtractor: {files.Length} JPEGs, {totalTags} tags, {sw.ElapsedMilliseconds}ms ({sw.ElapsedMilliseconds * 1000.0 / files.Length:F0}µs/file)");
    }

    // -----------------------------------------------------------------------
    // JPEG batch - exif-samples (89 files)
    // -----------------------------------------------------------------------

    [SkippableFact]
    public void Perf_Jpeg_ExifSamples_Sift()
    {
        Skip.IfNot(TestPaths.HasExifSamples, "testdata not available");
        var files = System.IO.Directory.GetFiles(TestPaths.ExifSamples, "*.jpg", SearchOption.AllDirectories);
        Skip.If(files.Length == 0, "no JPEG files");

        using (var f = SiftFile.Open(files[0])) { using var d = f.Parse(); _ = d.Tags(); }

        var sw = Stopwatch.StartNew();
        int totalTags = 0;
        foreach (var path in files)
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                totalTags += doc.Tags().Length;
            }
            catch (SiftException) { }
        }
        sw.Stop();

        output.WriteLine($"Sift: {files.Length} JPEGs, {totalTags} tags, {sw.ElapsedMilliseconds}ms ({sw.ElapsedMilliseconds * 1000.0 / files.Length:F0}µs/file)");
    }

    [SkippableFact]
    public void Perf_Jpeg_ExifSamples_MetadataExtractor()
    {
        Skip.IfNot(TestPaths.HasExifSamples, "testdata not available");
        var files = System.IO.Directory.GetFiles(TestPaths.ExifSamples, "*.jpg", SearchOption.AllDirectories);
        Skip.If(files.Length == 0, "no JPEG files");

        _ = ImageMetadataReader.ReadMetadata(files[0]);

        var sw = Stopwatch.StartNew();
        int totalTags = 0;
        foreach (var path in files)
        {
            try
            {
                var dirs = ImageMetadataReader.ReadMetadata(path);
                totalTags += dirs.Sum(d => d.TagCount);
            }
            catch { }
        }
        sw.Stop();

        output.WriteLine($"MetadataExtractor: {files.Length} JPEGs, {totalTags} tags, {sw.ElapsedMilliseconds}ms ({sw.ElapsedMilliseconds * 1000.0 / files.Length:F0}µs/file)");
    }

    // -----------------------------------------------------------------------
    // Single file - Canon.jpg deep extraction
    // -----------------------------------------------------------------------

    [SkippableFact]
    public void Perf_SingleFile_Canon_Sift()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");
        var path = Path.Combine(TestPaths.ExifToolImages, "Canon.jpg");

        // Warmup
        using (var f = SiftFile.Open(path)) { using var d = f.Parse(); _ = d.Tags(); }

        const int iterations = 1000;
        var sw = Stopwatch.StartNew();
        for (int i = 0; i < iterations; i++)
        {
            using var file = SiftFile.Open(path);
            using var doc = file.Parse();
            _ = doc.Tags();
        }
        sw.Stop();

        output.WriteLine($"Sift: Canon.jpg x{iterations}, {sw.ElapsedMilliseconds}ms ({sw.ElapsedMilliseconds * 1000.0 / iterations:F0}µs/iter)");
    }

    [SkippableFact]
    public void Perf_SingleFile_Canon_MetadataExtractor()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");
        var path = Path.Combine(TestPaths.ExifToolImages, "Canon.jpg");

        _ = ImageMetadataReader.ReadMetadata(path);

        const int iterations = 1000;
        var sw = Stopwatch.StartNew();
        for (int i = 0; i < iterations; i++)
        {
            _ = ImageMetadataReader.ReadMetadata(path);
        }
        sw.Stop();

        output.WriteLine($"MetadataExtractor: Canon.jpg x{iterations}, {sw.ElapsedMilliseconds}ms ({sw.ElapsedMilliseconds * 1000.0 / iterations:F0}µs/iter)");
    }

    // -----------------------------------------------------------------------
    // Buffer read - no filesystem overhead
    // -----------------------------------------------------------------------

    [SkippableFact]
    public void Perf_BufferRead_Sift()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");
        var data = File.ReadAllBytes(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));

        // Warmup
        using (var d = SiftLib.Read(data)) { _ = d.Tags(); }

        const int iterations = 1000;
        var sw = Stopwatch.StartNew();
        for (int i = 0; i < iterations; i++)
        {
            using var doc = SiftLib.Read(data);
            _ = doc.Tags();
        }
        sw.Stop();

        output.WriteLine($"Sift buffer: Canon.jpg x{iterations}, {sw.ElapsedMilliseconds}ms ({sw.ElapsedMilliseconds * 1000.0 / iterations:F0}µs/iter)");
    }

    [SkippableFact]
    public void Perf_BufferRead_MetadataExtractor()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");
        var data = File.ReadAllBytes(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));

        // Warmup
        using (var ms = new MemoryStream(data)) { _ = ImageMetadataReader.ReadMetadata(ms); }

        const int iterations = 1000;
        var sw = Stopwatch.StartNew();
        for (int i = 0; i < iterations; i++)
        {
            using var ms = new MemoryStream(data);
            _ = ImageMetadataReader.ReadMetadata(ms);
        }
        sw.Stop();

        output.WriteLine($"MetadataExtractor buffer: Canon.jpg x{iterations}, {sw.ElapsedMilliseconds}ms ({sw.ElapsedMilliseconds * 1000.0 / iterations:F0}µs/iter)");
    }
}
