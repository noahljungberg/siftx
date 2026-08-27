namespace SiftX.Tests;

public class PdfTests
{
    [SkippableFact]
    public void TextPages_ExtractsText()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var path = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories).FirstOrDefault();
        Skip.If(path is null, "no PDF files found");

        using var file = SiftFile.Open(path);
        using var doc = file.Parse();
        var pages = doc.TextPages();

        // Just verify it works without crash
        _ = pages.Length;
    }

    [SkippableFact]
    public void Images_ExtractsFromPdf()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var paths = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories);
        Skip.If(paths.Length == 0, "no PDF files found");

        int totalImages = 0;
        foreach (var path in paths.Take(30))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var images = doc.Images();
                totalImages += images.Length;

                foreach (var img in images)
                {
                    // Stencil masks may have 1x1 dimensions; verify no corruption
                    Assert.True(img.Width <= 100_000, $"width {img.Width} too large - likely struct alignment issue");
                    Assert.True(img.Height <= 100_000, $"height {img.Height} too large");
                    Assert.NotEmpty(img.Extension);
                }
            }
            catch (SiftException)
            {
                // Some may fail
            }
        }
        // Some PDFs may not have images - just verify no crashes
    }

    [SkippableFact]
    public void Tags_PdfHasMetadata()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var paths = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories);
        Skip.If(paths.Length == 0, "no PDF files found");

        int withTags = 0;
        foreach (var path in paths.Take(10))
        {
            try
            {
                var tags = SiftLib.Tags(path);
                if (tags.Any(t => t.Group == "PDF")) withTags++;
            }
            catch (SiftException)
            {
                // Some may fail
            }
        }

        Assert.True(withTags > 0, "at least some PDFs should have PDF tags");
    }

    [SkippableFact]
    public void TextPages_NonPdf_ReturnsEmpty()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var data = File.ReadAllBytes(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = SiftLib.Read(data);
        var pages = doc.TextPages();

        Assert.Empty(pages);
    }

    [SkippableFact]
    public void Images_NonPdf_ReturnsEmpty()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var data = File.ReadAllBytes(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = SiftLib.Read(data);
        var images = doc.Images();

        Assert.Empty(images);
    }

    [SkippableFact]
    public void ExtractedImage_Properties()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        // Find a PDF that we know has images
        var paths = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories);
        ExtractedImage? found = null;

        foreach (var path in paths.Take(20))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var images = doc.Images();
                if (images.Length > 0)
                {
                    found = images[0];
                    break;
                }
            }
            catch (SiftException) { }
        }

        Skip.If(found is null, "no PDF with images found");

        Assert.True(found.Width > 0);
        Assert.True(found.Height > 0);
        Assert.True(found.BitsPerComponent > 0);
        Assert.True(found.Components > 0);
        Assert.True(found.Data.Length > 0);

        if (found.IsPassthrough)
        {
            Assert.True(found.Format is ImageFormat.Jpeg or ImageFormat.Jpeg2000);
        }
    }

    [SkippableFact]
    public void ScanPdfs_TextExtraction()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var paths = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories);
        Skip.If(paths.Length == 0, "no PDF files found");

        int parsed = 0;
        int withText = 0;

        foreach (var path in paths.Take(20))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                parsed++;
                var pages = doc.TextPages();
                if (pages.Any(p => p.Length > 0)) withText++;
            }
            catch (SiftException) { }
        }

        Assert.True(parsed > 0, "should parse at least some PDFs");
    }
}
