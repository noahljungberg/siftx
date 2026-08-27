namespace SiftX.Tests;

public class TextPagesRawTests
{
    [SkippableFact]
    public void TextPagesRaw_NonPdf_ReturnsEmpty()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var data = File.ReadAllBytes(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = SiftLib.Read(data);
        var pages = doc.TextPagesRaw();

        Assert.Empty(pages);
    }

    [SkippableFact]
    public void TextPagesRaw_ReturnsSamePageCount()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var paths = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories);
        Skip.If(paths.Length == 0, "no PDF files found");

        int compared = 0;
        foreach (var path in paths.Take(20))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var layout = doc.TextPages();
                var raw = doc.TextPagesRaw();

                // Both should return the same number of pages
                Assert.Equal(layout.Length, raw.Length);
                compared++;
            }
            catch (SiftException) { }
        }

        Assert.True(compared > 0, "should compare at least some PDFs");
    }

    [SkippableFact]
    public void TextPagesRaw_ExtractsText()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var paths = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories);
        Skip.If(paths.Length == 0, "no PDF files found");

        int withText = 0;
        foreach (var path in paths.Take(20))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var pages = doc.TextPagesRaw();

                if (pages.Any(p => p.Length > 0))
                    withText++;
            }
            catch (SiftException) { }
        }

        Assert.True(withText > 0, "at least some PDFs should have extractable text");
    }

    [SkippableFact]
    public void TextPagesRaw_NoCrashOnAnyPdf()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var paths = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories);
        Skip.If(paths.Length == 0, "no PDF files found");

        int parsed = 0;
        foreach (var path in paths)
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var pages = doc.TextPagesRaw();
                parsed++;

                foreach (var page in pages)
                {
                    // Each page is a string - should not be null
                    Assert.NotNull(page);
                }
            }
            catch (SiftException) { }
        }

        Assert.True(parsed > 0, "should parse at least some PDFs");
    }

    [SkippableFact]
    public void TextPagesRaw_VsLayout_SharedContent()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var paths = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories);
        Skip.If(paths.Length == 0, "no PDF files found");

        int verified = 0;
        foreach (var path in paths.Take(20))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var layout = doc.TextPages();
                var raw = doc.TextPagesRaw();

                for (int i = 0; i < layout.Length; i++)
                {
                    // If layout has text, raw should too (they share the same content)
                    if (layout[i].Length > 10)
                    {
                        Assert.True(raw[i].Length > 0,
                            $"page {i}: layout has text but raw is empty");
                        verified++;
                    }
                }
            }
            catch (SiftException) { }
        }
        // Just verify no inconsistencies were found
    }
}
