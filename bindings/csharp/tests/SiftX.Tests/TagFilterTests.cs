namespace SiftX.Tests;

public class TagFilterTests
{
    [SkippableFact]
    public void ExifTags_OnlyReturnsExifRelatedGroups()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();

        var exifTags = doc.ExifTags();
        Assert.NotEmpty(exifTags);
        // ExifTags returns tags from EXIF IFDs, which includes MakerNotes
        Assert.All(exifTags, t => Assert.True(
            t.Group is "EXIF" or "MakerNotes",
            $"unexpected group: {t.Group}"));
        // Must include core EXIF tags
        Assert.Contains(exifTags, t => t.Group == "EXIF");
        // Should not include XMP or IPTC
        Assert.DoesNotContain(exifTags, t => t.Group is "XMP" or "IPTC");
    }

    [SkippableFact]
    public void ExifTags_SubsetOfAllTags()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();

        var allTags = doc.Tags();
        var exifTags = doc.ExifTags();

        // Filtered set should be a subset
        Assert.True(exifTags.Length <= allTags.Length);

        // Every EXIF tag from the filtered call should exist in the full set
        foreach (var tag in exifTags)
        {
            Assert.Contains(allTags, t => t.Group == tag.Group && t.Name == tag.Name && t.Value == tag.Value);
        }
    }

    [SkippableFact]
    public void ExifTags_MatchesManualFilter()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();

        var exifTags = doc.ExifTags();
        var allTags = doc.Tags();
        // ExifTags includes EXIF + MakerNotes (both stored in EXIF IFDs)
        var manualFilter = allTags.Where(t => t.Group is "EXIF" or "MakerNotes").ToArray();

        Assert.Equal(manualFilter.Length, exifTags.Length);
        for (int i = 0; i < manualFilter.Length; i++)
        {
            Assert.Equal(manualFilter[i], exifTags[i]);
        }
    }

    [SkippableFact]
    public void XmpTags_OnlyReturnsXmpGroup()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        // Scan for a file that has XMP tags
        var jpegFiles = Directory.GetFiles(TestPaths.ExifToolImages, "*.jpg");
        Skip.If(jpegFiles.Length == 0, "no JPEG files found");

        foreach (var path in jpegFiles.Take(20))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var xmpTags = doc.XmpTags();

                // All returned tags must be XMP group
                Assert.All(xmpTags, t => Assert.Equal("XMP", t.Group));
            }
            catch (SiftException) { }
        }
    }

    [SkippableFact]
    public void XmpTags_SubsetOfAllTags()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var jpegFiles = Directory.GetFiles(TestPaths.ExifToolImages, "*.jpg");
        Skip.If(jpegFiles.Length == 0, "no JPEG files found");

        int verified = 0;
        foreach (var path in jpegFiles.Take(20))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var allTags = doc.Tags();
                var xmpTags = doc.XmpTags();

                Assert.True(xmpTags.Length <= allTags.Length);

                foreach (var tag in xmpTags)
                {
                    Assert.Contains(allTags,
                        t => t.Group == tag.Group && t.Name == tag.Name && t.Value == tag.Value);
                }
                verified++;
            }
            catch (SiftException) { }
        }

        Assert.True(verified > 0, "should verify at least some files");
    }

    [SkippableFact]
    public void IptcTags_OnlyReturnsIptcGroup()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var jpegFiles = Directory.GetFiles(TestPaths.ExifToolImages, "*.jpg");
        Skip.If(jpegFiles.Length == 0, "no JPEG files found");

        foreach (var path in jpegFiles.Take(20))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var iptcTags = doc.IptcTags();

                Assert.All(iptcTags, t => Assert.Equal("IPTC", t.Group));
            }
            catch (SiftException) { }
        }
    }

    [SkippableFact]
    public void IptcTags_SubsetOfAllTags()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var jpegFiles = Directory.GetFiles(TestPaths.ExifToolImages, "*.jpg");
        Skip.If(jpegFiles.Length == 0, "no JPEG files found");

        int verified = 0;
        foreach (var path in jpegFiles.Take(20))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var allTags = doc.Tags();
                var iptcTags = doc.IptcTags();

                Assert.True(iptcTags.Length <= allTags.Length);

                foreach (var tag in iptcTags)
                {
                    Assert.Contains(allTags,
                        t => t.Group == tag.Group && t.Name == tag.Name && t.Value == tag.Value);
                }
                verified++;
            }
            catch (SiftException) { }
        }

        Assert.True(verified > 0, "should verify at least some files");
    }

    [SkippableFact]
    public void TagFilters_NonPdf_NoXmpCrash()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var pngPath = Directory.GetFiles(TestPaths.ExifToolImages, "*.png").FirstOrDefault();
        Skip.If(pngPath is null, "no PNG files found");

        using var file = SiftFile.Open(pngPath);
        using var doc = file.Parse();

        // All three filter APIs should work without crashing on any format
        _ = doc.ExifTags();
        _ = doc.XmpTags();
        _ = doc.IptcTags();
    }

    [SkippableFact]
    public void TagFilters_PdfFile()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var path = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories).FirstOrDefault();
        Skip.If(path is null, "no PDF files found");

        using var file = SiftFile.Open(path);
        using var doc = file.Parse();

        var exif = doc.ExifTags();
        var xmp = doc.XmpTags();
        var iptc = doc.IptcTags();

        // PDFs typically don't have EXIF/IPTC, but should not crash
        Assert.All(exif, t => Assert.Equal("EXIF", t.Group));
        Assert.All(xmp, t => Assert.Equal("XMP", t.Group));
        Assert.All(iptc, t => Assert.Equal("IPTC", t.Group));
    }

    [SkippableFact]
    public void TagFilters_BufferPath()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var data = File.ReadAllBytes(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = SiftLib.Read(data);

        var exifTags = doc.ExifTags();
        Assert.NotEmpty(exifTags);
        Assert.All(exifTags, t => Assert.True(
            t.Group is "EXIF" or "MakerNotes",
            $"unexpected group: {t.Group}"));
        Assert.DoesNotContain(exifTags, t => t.Group is "XMP" or "IPTC");

        // XMP and IPTC should also work via buffer path
        var xmpTags = doc.XmpTags();
        Assert.All(xmpTags, t => Assert.Equal("XMP", t.Group));

        var iptcTags = doc.IptcTags();
        Assert.All(iptcTags, t => Assert.Equal("IPTC", t.Group));
    }
}
