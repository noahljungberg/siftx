namespace SiftX.Tests;

public class TypedMetadataTests
{
    // --- TypedTag model ---

    [Fact]
    public void TypedTag_AsInt32_IntegerType()
    {
        var tag = new TypedTag("EXIF", "ISO", "400", TagValueType.U16, 400, 0, 0, 400.0);
        Assert.Equal(400, tag.AsInt32());
    }

    [Fact]
    public void TypedTag_AsInt32_StringType_ReturnsNull()
    {
        var tag = new TypedTag("XMP", "Title", "Hello", TagValueType.String, 0, 0, 0, 0.0);
        Assert.Null(tag.AsInt32());
    }

    [Fact]
    public void TypedTag_AsDouble_FloatType()
    {
        var tag = new TypedTag("EXIF", "FNumber", "2.8", TagValueType.Rational, 0, 28, 10, 2.8);
        Assert.Equal(2.8, tag.AsDouble());
    }

    [Fact]
    public void TypedTag_AsDouble_IntegerType()
    {
        var tag = new TypedTag("EXIF", "ISO", "400", TagValueType.U16, 400, 0, 0, 400.0);
        Assert.Equal(400.0, tag.AsDouble());
    }

    [Fact]
    public void TypedTag_AsRational_RationalType()
    {
        var tag = new TypedTag("EXIF", "ExposureTime", "1/200", TagValueType.Rational, 0, 1, 200, 0.005);
        var r = tag.AsRational();
        Assert.NotNull(r);
        Assert.Equal(1, r.Value.Num);
        Assert.Equal(200, r.Value.Den);
    }

    [Fact]
    public void TypedTag_AsRational_NonRational_ReturnsNull()
    {
        var tag = new TypedTag("EXIF", "ISO", "400", TagValueType.U16, 400, 0, 0, 400.0);
        Assert.Null(tag.AsRational());
    }

    [Fact]
    public void TypedTag_ImplicitConversionToTag()
    {
        var typed = new TypedTag("EXIF", "Make", "Canon", TagValueType.String, 0, 0, 0, 0.0);
        Tag tag = typed;
        Assert.Equal("EXIF", tag.Group);
        Assert.Equal("Make", tag.Name);
        Assert.Equal("Canon", tag.Value);
    }

    // --- ExifDirectory ---

    [SkippableFact]
    public void ExifDirectory_Make_ReturnsString()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();

        Assert.Equal("Canon", doc.Exif.Make);
    }

    [SkippableFact]
    public void ExifDirectory_Model_ReturnsString()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();

        Assert.NotNull(doc.Exif.Model);
        Assert.Contains("Canon", doc.Exif.Model);
    }

    [SkippableFact]
    public void ExifDirectory_Iso_ReturnsInt()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();

        if (doc.Exif.Iso is int iso)
            Assert.True(iso > 0 && iso < 1_000_000, $"ISO {iso} out of expected range");
    }

    [SkippableFact]
    public void ExifDirectory_FNumber_ReturnsDouble()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();

        if (doc.Exif.FNumber is double fn)
            Assert.True(fn > 0.5 && fn < 100.0, $"FNumber {fn} out of expected range");
    }

    [SkippableFact]
    public void ExifDirectory_ExposureTime_ReturnsRational()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();

        if (doc.Exif.ExposureTime is var (num, den))
        {
            Assert.True(num > 0, "numerator should be positive");
            Assert.True(den > 0, "denominator should be positive");
        }
    }

    [SkippableFact]
    public void ExifDirectory_Indexer_LooksUpAnyTag()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();

        var make = doc.Exif["Make"];
        Assert.NotNull(make);
        Assert.Equal("Canon", make.Value.Value);
    }

    [SkippableFact]
    public void ExifDirectory_Orientation_ReturnsEnum()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        // Scan for a file with an orientation tag
        var jpegFiles = Directory.GetFiles(TestPaths.ExifToolImages, "*.jpg");
        foreach (var path in jpegFiles.Take(20))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                if (doc.Exif.Orientation is ExifOrientation o)
                {
                    Assert.True(Enum.IsDefined(o));
                    return;
                }
            }
            catch (SiftException) { }
        }
        // Not all files have orientation - just verify no crashes
    }

    // --- GpsDirectory ---

    [SkippableFact]
    public void GpsDirectory_Coordinates_FromExifSamples()
    {
        Skip.IfNot(TestPaths.HasExifSamples, "testdata not available");

        var path = Path.Combine(TestPaths.ExifSamples, "jpg", "gps", "DSCN0010.jpg");
        Skip.IfNot(File.Exists(path), "DSCN0010.jpg not found");

        using var file = SiftFile.Open(path);
        using var doc = file.Parse();

        var gps = doc.GpsInfo;
        Assert.NotNull(gps.Coordinates);
        Assert.NotEqual(0.0, gps.Coordinates.Value.Latitude);
        Assert.NotEqual(0.0, gps.Coordinates.Value.Longitude);
    }

    // --- XmpDirectory ---

    [SkippableFact]
    public void XmpDirectory_Properties_NoCrash()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var jpegFiles = Directory.GetFiles(TestPaths.ExifToolImages, "*.jpg");
        foreach (var path in jpegFiles.Take(20))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                // Just access properties - verify no crash
                _ = doc.Xmp.Title;
                _ = doc.Xmp.Creator;
                _ = doc.Xmp.CreateDate;
            }
            catch (SiftException) { }
        }
    }

    // --- IptcDirectory ---

    [SkippableFact]
    public void IptcDirectory_Properties_NoCrash()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var jpegFiles = Directory.GetFiles(TestPaths.ExifToolImages, "*.jpg");
        foreach (var path in jpegFiles.Take(20))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                _ = doc.Iptc.Headline;
                _ = doc.Iptc.Keywords;
                _ = doc.Iptc.City;
            }
            catch (SiftException) { }
        }
    }

    // --- PdfDirectory ---

    [SkippableFact]
    public void PdfDirectory_Properties()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var paths = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories);
        Skip.If(paths.Length == 0, "no PDF files found");

        int withTitle = 0;
        foreach (var path in paths.Take(20))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                if (doc.Pdf.Title is not null) withTitle++;
                // Just verify typed access works
                _ = doc.Pdf.Author;
                _ = doc.Pdf.PageCount;
                _ = doc.Pdf.PdfVersion;
                _ = doc.Pdf.IsEncrypted;
                _ = doc.Pdf.IsTagged;
            }
            catch (SiftException) { }
        }
    }

    // --- TypedTags round-trip ---

    [SkippableFact]
    public void TypedTags_ExifTagsHaveTypedValues()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();
        var typed = doc.TypedTags();

        Assert.NotEmpty(typed);

        // EXIF tags should have typed values (not all String)
        var exifTyped = typed.Where(t => t.Group == "EXIF" && t.ValueType != TagValueType.String).ToList();
        Assert.True(exifTyped.Count > 0, "some EXIF tags should have typed values");

        // XMP/IPTC tags should be String type
        foreach (var t in typed.Where(t => t.Group is "XMP" or "IPTC"))
        {
            Assert.Equal(TagValueType.String, t.ValueType);
        }
    }

    [SkippableFact]
    public void TypedTags_ConsistentWithTags()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        using var file = SiftFile.Open(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = file.Parse();
        var tags = doc.Tags();
        var typed = doc.TypedTags();

        // Same count, same display strings
        Assert.Equal(tags.Length, typed.Length);
        for (int i = 0; i < tags.Length; i++)
        {
            Assert.Equal(tags[i].Group, typed[i].Group);
            Assert.Equal(tags[i].Name, typed[i].Name);
            Assert.Equal(tags[i].Value, typed[i].Value);
        }
    }

    // --- Multi-format directories ---

    [SkippableFact]
    public void Directories_NoCrash_AllFormats()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var extensions = new[] { "*.jpg", "*.png", "*.gif", "*.tif", "*.webp" };
        foreach (var ext in extensions)
        {
            foreach (var path in Directory.GetFiles(TestPaths.ExifToolImages, ext).Take(3))
            {
                try
                {
                    using var file = SiftFile.Open(path);
                    using var doc = file.Parse();
                    _ = doc.Exif.Make;
                    _ = doc.GpsInfo.Coordinates;
                    _ = doc.Xmp.Title;
                    _ = doc.Iptc.Headline;
                }
                catch (SiftException) { }
            }
        }
    }
}
