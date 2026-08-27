namespace SiftX.Tests;

public class AnnotationTests
{
    [SkippableFact]
    public void Annotations_NonPdf_ReturnsEmpty()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var data = File.ReadAllBytes(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = SiftLib.Read(data);
        var annots = doc.Annotations();

        Assert.Empty(annots);
    }

    [SkippableFact]
    public void Annotations_NoCrashOnAnyPdf()
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
                var annots = doc.Annotations();
                parsed++;

                foreach (var annot in annots)
                {
                    Assert.True(Enum.IsDefined(annot.AnnotationType),
                        $"unexpected AnnotationType enum value: {annot.AnnotationType}");
                }
            }
            catch (SiftException) { }
        }

        Assert.True(parsed > 0, "should parse at least some PDFs");
    }

    [SkippableFact]
    public void Annotations_RectIsValid()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var paths = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories);
        Skip.If(paths.Length == 0, "no PDF files found");

        foreach (var path in paths.Take(30))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var annots = doc.Annotations();

                foreach (var annot in annots)
                {
                    // All coordinates should be finite
                    Assert.False(double.IsNaN(annot.Rect.Llx));
                    Assert.False(double.IsNaN(annot.Rect.Lly));
                    Assert.False(double.IsNaN(annot.Rect.Urx));
                    Assert.False(double.IsNaN(annot.Rect.Ury));
                    Assert.False(double.IsInfinity(annot.Rect.Llx));
                    Assert.False(double.IsInfinity(annot.Rect.Ury));
                }
            }
            catch (SiftException) { }
        }
    }

    [SkippableFact]
    public void Annotations_ScanPdfjsCorpus()
    {
        Skip.IfNot(TestPaths.HasPdfjsPdfs, "testdata not available");

        var paths = Directory.GetFiles(TestPaths.PdfjsPdfs, "*.pdf", SearchOption.AllDirectories);
        Skip.If(paths.Length == 0, "no PDF files found");

        int withAnnots = 0;
        int totalAnnots = 0;
        foreach (var path in paths.Take(100))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var annots = doc.Annotations();
                if (annots.Length > 0)
                {
                    withAnnots++;
                    totalAnnots += annots.Length;
                }
            }
            catch (SiftException) { }
        }
        // Many PDFs have link annotations - expect some hits from a large corpus
    }

    [SkippableFact]
    public void Annotations_TypeIsNotUnknown_ForCommonTypes()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var paths = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories);
        Skip.If(paths.Length == 0, "no PDF files found");

        int known = 0;
        int total = 0;
        foreach (var path in paths.Take(30))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var annots = doc.Annotations();

                foreach (var annot in annots)
                {
                    total++;
                    if (annot.AnnotationType != AnnotationType.Unknown)
                        known++;
                }
            }
            catch (SiftException) { }
        }
        // Most annotations in well-formed PDFs should have a known type
        if (total > 0)
            Assert.True(known > 0, "at least some annotations should have known types");
    }

    [Fact]
    public void AnnotationInfo_RecordEquality()
    {
        var rect = new PdfRect(0.0, 0.0, 100.0, 50.0);
        var a = new AnnotationInfo(AnnotationType.Link, 0, rect, null, "http://example.com", AnnotationFlags.None, true);
        var b = new AnnotationInfo(AnnotationType.Link, 0, rect, null, "http://example.com", AnnotationFlags.None, true);
        var c = new AnnotationInfo(AnnotationType.Text, 1, new PdfRect(10, 10, 200, 100), "Note", null, AnnotationFlags.Print, false);

        Assert.Equal(a, b);
        Assert.NotEqual(a, c);
    }

    [Fact]
    public void PdfRect_Properties()
    {
        var rect = new PdfRect(10.0, 20.0, 110.0, 70.0);
        Assert.Equal(100.0, rect.Width);
        Assert.Equal(50.0, rect.Height);
        Assert.Equal(10.0, rect.Llx);
        Assert.Equal(20.0, rect.Lly);
        Assert.Equal(110.0, rect.Urx);
        Assert.Equal(70.0, rect.Ury);
    }

    [Fact]
    public void PdfRect_ToString_Format()
    {
        var rect = new PdfRect(0.0, 0.0, 612.0, 792.0);
        Assert.Equal("[0.0, 0.0, 612.0, 792.0]", rect.ToString());
    }

    [Fact]
    public void PdfRect_RecordEquality()
    {
        var a = new PdfRect(0, 0, 100, 50);
        var b = new PdfRect(0, 0, 100, 50);
        var c = new PdfRect(10, 10, 200, 100);

        Assert.Equal(a, b);
        Assert.NotEqual(a, c);
    }

    [Fact]
    public void AnnotationFlags_HasExpectedBits()
    {
        var flags = AnnotationFlags.Print | AnnotationFlags.Locked;
        Assert.True(flags.HasFlag(AnnotationFlags.Print));
        Assert.True(flags.HasFlag(AnnotationFlags.Locked));
        Assert.False(flags.HasFlag(AnnotationFlags.Hidden));
    }
}
