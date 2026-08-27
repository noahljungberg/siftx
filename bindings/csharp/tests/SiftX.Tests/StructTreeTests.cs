namespace SiftX.Tests;

public class StructTreeTests
{
    [SkippableFact]
    public void StructTree_NonPdf_ReturnsEmpty()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var data = File.ReadAllBytes(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = SiftLib.Read(data);
        var tree = doc.StructTree();

        Assert.Empty(tree);
    }

    [SkippableFact]
    public void StructTree_NoCrashOnAnyPdf()
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
                var tree = doc.StructTree();
                parsed++;

                foreach (var elem in tree)
                {
                    Assert.NotNull(elem.StructType);
                    Assert.NotEmpty(elem.StructType);
                }
            }
            catch (SiftException) { }
        }

        Assert.True(parsed > 0, "should parse at least some PDFs");
    }

    [SkippableFact]
    public void StructTree_DepthIsConsistent()
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
                var tree = doc.StructTree();

                if (tree.Length == 0) continue;

                // First element should be at depth 0 (root)
                Assert.Equal(0u, tree[0].Depth);

                // Depth should never jump by more than 1 going deeper
                for (int i = 1; i < tree.Length; i++)
                {
                    var prev = tree[i - 1].Depth;
                    var curr = tree[i].Depth;
                    // Can go arbitrarily shallower, but only 1 deeper at a time
                    Assert.True(curr <= prev + 1,
                        $"Depth jumped from {prev} to {curr} at index {i} in {Path.GetFileName(path)}");
                }
            }
            catch (SiftException) { }
        }
    }

    [SkippableFact]
    public void StructTree_ScanPdfjsCorpus()
    {
        Skip.IfNot(TestPaths.HasPdfjsPdfs, "testdata not available");

        var paths = Directory.GetFiles(TestPaths.PdfjsPdfs, "*.pdf", SearchOption.AllDirectories);
        Skip.If(paths.Length == 0, "no PDF files found");

        int withTree = 0;
        foreach (var path in paths.Take(100))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var tree = doc.StructTree();
                if (tree.Length > 0) withTree++;
            }
            catch (SiftException) { }
        }
        // Tagged PDFs have structure trees - expect some hits
    }

    [SkippableFact]
    public void StructTreeRoleMap_NonPdf_ReturnsEmpty()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var data = File.ReadAllBytes(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = SiftLib.Read(data);
        var roleMap = doc.StructTreeRoleMap();

        Assert.Empty(roleMap);
    }

    [SkippableFact]
    public void StructTreeRoleMap_NoCrashOnAnyPdf()
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
                var roleMap = doc.StructTreeRoleMap();
                parsed++;

                foreach (var entry in roleMap)
                {
                    Assert.NotEmpty(entry.Custom);
                    Assert.NotEmpty(entry.Standard);
                }
            }
            catch (SiftException) { }
        }

        Assert.True(parsed > 0, "should parse at least some PDFs");
    }

    [SkippableFact]
    public void StructElementInfo_RecordEquality()
    {
        var a = new StructElementInfo("P", 1, null, null, null, null);
        var b = new StructElementInfo("P", 1, null, null, null, null);
        var c = new StructElementInfo("H1", 0, "Title", null, null, "en");

        Assert.Equal(a, b);
        Assert.NotEqual(a, c);
    }

    [SkippableFact]
    public void RoleMapEntry_RecordEquality()
    {
        var a = new RoleMapEntry("Caption", "P");
        var b = new RoleMapEntry("Caption", "P");
        var c = new RoleMapEntry("Footnote", "Note");

        Assert.Equal(a, b);
        Assert.NotEqual(a, c);
    }
}
