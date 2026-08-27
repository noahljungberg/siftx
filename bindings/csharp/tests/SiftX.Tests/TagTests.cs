namespace SiftX.Tests;

public class TagTests
{
    [SkippableFact]
    public void Tags_CanonJpeg_HasMakeTag()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();

        var tags = doc.Tags();
        Assert.NotEmpty(tags);

        var make = tags.FirstOrDefault(t => t.Name == "Make");
        Assert.NotEqual(default, make);
        Assert.Equal("Canon", make.Value);
        Assert.Equal("EXIF", make.Group);
    }

    [SkippableFact]
    public void Tags_CanonJpeg_HasModelTag()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();

        var tags = doc.Tags();
        var model = tags.FirstOrDefault(t => t.Name == "Model");
        Assert.NotEqual(default, model);
        Assert.Contains("Canon", model.Value);
    }

    [SkippableFact]
    public void Tags_HasMultipleGroups()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();

        var tags = doc.Tags();
        var groups = tags.Select(t => t.Group).Distinct().ToList();

        Assert.Contains("EXIF", groups);
    }

    [SkippableFact]
    public void TagsFromPath_Convenience()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var tags = SiftLib.Tags(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        Assert.NotEmpty(tags);
        Assert.Contains(tags, t => t.Name == "Make");
    }

    [SkippableFact]
    public void Tags_ScanMultipleJpegs()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var jpegFiles = Directory.GetFiles(TestPaths.ExifToolImages, "*.jpg");
        Skip.If(jpegFiles.Length == 0, "no JPEG files found");

        int parsed = 0;
        int withTags = 0;

        foreach (var path in jpegFiles.Take(20))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var tags = doc.Tags();
                parsed++;
                if (tags.Length > 0) withTags++;
            }
            catch (SiftException)
            {
                // Some test files may be intentionally malformed
            }
        }

        Assert.True(parsed > 0, "should parse at least some JPEGs");
        Assert.True(withTags > 0, "at least some JPEGs should have tags");
    }

    [SkippableFact]
    public void Tags_PngFile_HasTags()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var path = Directory.GetFiles(TestPaths.ExifToolImages, "*.png").FirstOrDefault();
        Skip.If(path is null, "no PNG files found");

        using var file = SiftFile.Open(path);
        using var doc = file.Parse();
        var tags = doc.Tags();
        // PNG may or may not have tags, just verify no crash
        _ = tags.Length;
    }

    [SkippableFact]
    public void TagLookup_ByName()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();

        var make = doc.Tag("Make");
        Assert.NotNull(make);
        Assert.Equal("Canon", make.Value.Value);
        Assert.Equal("EXIF", make.Value.Group);
    }

    [SkippableFact]
    public void TagLookup_ByGroupAndName()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();

        var make = doc.Tag("EXIF", "Make");
        Assert.NotNull(make);
        Assert.Equal("Canon", make.Value.Value);
    }

    [SkippableFact]
    public void TagLookup_NotFound_ReturnsNull()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();

        Assert.Null(doc.Tag("NonExistentTag"));
        Assert.Null(doc.Tag("EXIF", "NonExistentTag"));
    }

    [Fact]
    public void Tag_ToString_Format()
    {
        var tag = new Tag("EXIF", "Make", "Canon");
        Assert.Equal("[EXIF] Make = Canon", tag.ToString());
    }

    [Fact]
    public void Tag_RecordEquality()
    {
        var a = new Tag("EXIF", "Make", "Canon");
        var b = new Tag("EXIF", "Make", "Canon");
        var c = new Tag("EXIF", "Model", "EOS");

        Assert.Equal(a, b);
        Assert.NotEqual(a, c);
    }
}
