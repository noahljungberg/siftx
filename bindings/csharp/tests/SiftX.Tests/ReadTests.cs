namespace SiftX.Tests;

public class ReadTests
{
    [SkippableFact]
    public void Read_FromByteArray()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var data = File.ReadAllBytes(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = SiftLib.Read(data);

        Assert.Equal(FileType.Jpeg, doc.FileType);
        var tags = doc.Tags();
        Assert.NotEmpty(tags);
        Assert.Contains(tags, t => t.Name == "Make" && t.Value == "Canon");
    }

    [SkippableFact]
    public void Read_FromSpan()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var data = File.ReadAllBytes(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        ReadOnlySpan<byte> span = data;

        using var doc = SiftLib.Read(span);
        Assert.Equal(FileType.Jpeg, doc.FileType);
    }

    [SkippableFact]
    public void Read_Pdf_FromBuffer()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var path = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories).FirstOrDefault();
        Skip.If(path is null, "no PDF files found");

        var data = File.ReadAllBytes(path);
        using var doc = SiftLib.Read(data);

        Assert.Equal(FileType.Pdf, doc.FileType);
    }

    [SkippableFact]
    public void Read_Tags_SameAsOpen()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var filePath = Path.Combine(TestPaths.ExifToolImages, "Canon.jpg");
        var data = File.ReadAllBytes(filePath);

        // Via siftx_read (buffer)
        using var docFromRead = SiftLib.Read(data);
        var tagsFromRead = docFromRead.Tags();

        // Via siftx_open + siftx_parse (mmap)
        using var file = SiftFile.Open(filePath);
        using var docFromOpen = file.Parse();
        var tagsFromOpen = docFromOpen.Tags();

        // Same tags either way
        Assert.Equal(tagsFromOpen.Length, tagsFromRead.Length);
        for (int i = 0; i < tagsFromOpen.Length; i++)
        {
            Assert.Equal(tagsFromOpen[i].Group, tagsFromRead[i].Group);
            Assert.Equal(tagsFromOpen[i].Name, tagsFromRead[i].Name);
            Assert.Equal(tagsFromOpen[i].Value, tagsFromRead[i].Value);
        }
    }
}
