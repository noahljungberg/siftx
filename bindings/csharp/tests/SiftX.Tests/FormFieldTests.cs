namespace SiftX.Tests;

public class FormFieldTests
{
    [SkippableFact]
    public void FormFields_NonPdf_ReturnsEmpty()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var data = File.ReadAllBytes(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = SiftLib.Read(data);
        var fields = doc.FormFields();

        Assert.Empty(fields);
    }

    [SkippableFact]
    public void FormFields_NoCrashOnAnyPdf()
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
                var fields = doc.FormFields();
                parsed++;

                foreach (var field in fields)
                {
                    Assert.NotNull(field.Name);
                    Assert.True(Enum.IsDefined(field.FieldType),
                        $"unexpected FieldType enum value: {field.FieldType}");
                }
            }
            catch (SiftException)
            {
                // Some PDFs may fail to parse
            }
        }

        Assert.True(parsed > 0, "should parse at least some PDFs");
    }

    [SkippableFact]
    public void FormFields_ScanForFieldProperties()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var paths = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories);
        Skip.If(paths.Length == 0, "no PDF files found");

        int totalFields = 0;
        foreach (var path in paths)
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var fields = doc.FormFields();
                totalFields += fields.Length;

                foreach (var field in fields)
                {
                    // Name should not be null (may be empty for some fields)
                    Assert.NotNull(field.Name);

                    // ReadOnly/Required should be consistent with Flags
                    if (field.IsReadOnly)
                        Assert.True(field.Flags.HasFlag(FormFieldFlags.ReadOnly));
                    if (field.IsRequired)
                        Assert.True(field.Flags.HasFlag(FormFieldFlags.Required));
                }
            }
            catch (SiftException) { }
        }
        // Just verify the scan completed - not all corpora have form PDFs
    }

    [SkippableFact]
    public void FormFields_ScanPdfjsCorpus()
    {
        Skip.IfNot(TestPaths.HasPdfjsPdfs, "testdata not available");

        var paths = Directory.GetFiles(TestPaths.PdfjsPdfs, "*.pdf", SearchOption.AllDirectories);
        Skip.If(paths.Length == 0, "no PDF files found");

        int withFields = 0;
        foreach (var path in paths.Take(100))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var fields = doc.FormFields();
                if (fields.Length > 0) withFields++;
            }
            catch (SiftException) { }
        }
        // Larger corpus - more likely to have form PDFs
    }

    [Fact]
    public void FormFieldInfo_RecordEquality()
    {
        var a = new FormFieldInfo(FormFieldType.Text, "name", "val", null, FormFieldFlags.None, false, true);
        var b = new FormFieldInfo(FormFieldType.Text, "name", "val", null, FormFieldFlags.None, false, true);
        var c = new FormFieldInfo(FormFieldType.Button, "submit", null, null, FormFieldFlags.None, false, false);

        Assert.Equal(a, b);
        Assert.NotEqual(a, c);
    }

    [Fact]
    public void FormFieldFlags_HasExpectedBits()
    {
        var flags = FormFieldFlags.ReadOnly | FormFieldFlags.Required;
        Assert.True(flags.HasFlag(FormFieldFlags.ReadOnly));
        Assert.True(flags.HasFlag(FormFieldFlags.Required));
        Assert.False(flags.HasFlag(FormFieldFlags.Multiline));
    }

    [Fact]
    public void FormFieldType_Enum_IsDefined()
    {
        Assert.True(Enum.IsDefined(FormFieldType.Text));
        Assert.True(Enum.IsDefined(FormFieldType.Button));
        Assert.True(Enum.IsDefined(FormFieldType.Choice));
        Assert.True(Enum.IsDefined(FormFieldType.Signature));
        Assert.True(Enum.IsDefined(FormFieldType.Unknown));
    }
}
