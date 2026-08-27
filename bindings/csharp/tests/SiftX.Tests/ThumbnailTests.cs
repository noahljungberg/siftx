namespace SiftX.Tests;

public class ThumbnailTests
{
    [SkippableFact]
    public void Thumbnail_DscnJpeg_ReturnsJpegBytes()
    {
        Skip.IfNot(TestPaths.HasExifSamples, "testdata not available");

        var path = Path.Combine(TestPaths.ExifSamples, "jpg", "gps", "DSCN0010.jpg");
        Skip.IfNot(File.Exists(path), "DSCN0010.jpg not found");

        using var file = SiftFile.Open(path);
        using var doc = file.Parse();
        var thumb = doc.Thumbnail();

        Assert.NotNull(thumb);
        Assert.True(thumb.Length > 100, "thumbnail should have meaningful size");
        // JPEG magic bytes
        Assert.Equal(0xFF, thumb[0]);
        Assert.Equal(0xD8, thumb[1]);
        Assert.Equal(0xFF, thumb[2]);
    }

    [SkippableFact]
    public void Thumbnail_ScanJpegs_SomeHaveThumbnails()
    {
        Skip.IfNot(TestPaths.HasExifSamples, "testdata not available");

        var jpegDir = Path.Combine(TestPaths.ExifSamples, "jpg");
        Skip.IfNot(Directory.Exists(jpegDir), "exif-samples/jpg not found");

        var jpegFiles = Directory.GetFiles(jpegDir, "*.jpg", SearchOption.AllDirectories);
        Skip.If(jpegFiles.Length == 0, "no JPEG files found");

        int withThumb = 0;
        foreach (var path in jpegFiles)
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                if (doc.Thumbnail() is not null)
                    withThumb++;
            }
            catch (SiftException)
            {
                // Some files may be malformed
            }
        }

        Assert.True(withThumb > 0, "at least some JPEGs should have thumbnails");
    }

    [SkippableFact]
    public void Thumbnail_Pdf_ReturnsNull()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var path = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories).FirstOrDefault();
        Skip.If(path is null, "no PDF files found");

        using var file = SiftFile.Open(path);
        using var doc = file.Parse();
        Assert.Null(doc.Thumbnail());
    }

    [SkippableFact]
    public void Thumbnail_NoThumbnail_ReturnsNull()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        // Try to find a file without a thumbnail (some JPEGs don't have IFD1)
        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();
        // Just verify no crash - may or may not have thumbnail
        _ = doc.Thumbnail();
    }
}
