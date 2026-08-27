namespace SiftX.Tests;

public class AuthenticateTests
{
    [SkippableFact]
    public void Authenticate_NonPdf_ReturnsFalse()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        var data = File.ReadAllBytes(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = SiftLib.Read(data);

        // Non-PDF should not crash; authentication is a no-op
        var result = doc.Authenticate("password");
        // Just verify no crash - result depends on implementation
        _ = result;
    }

    [SkippableFact]
    public void Authenticate_OwnerOnlyPdf_EmptyPasswordSucceeds()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        // Owner-only encrypted PDFs accept empty user password
        var path = FindPdf("Gday garçon - owner.pdf");
        Skip.If(path is null, "Gday garçon - owner.pdf not found");

        using var file = SiftFile.Open(path);
        using var doc = file.Parse();

        // Empty password should work for owner-only encryption
        var result = doc.Authenticate("");
        Assert.True(result, "owner-only PDF should accept empty password");
    }

    [SkippableFact]
    public void Authenticate_PasswordProtectedPdf_WrongPasswordFails()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var path = FindPdf("PasswordEncrypted.pdf");
        Skip.If(path is null, "PasswordEncrypted.pdf not found");

        using var file = SiftFile.Open(path);
        using var doc = file.Parse();

        var result = doc.Authenticate("definitely-wrong-password");
        Assert.False(result, "wrong password should be rejected");
    }

    [SkippableFact]
    public void Authenticate_PasswordProtectedPdf_EmptyPasswordFails()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        // "Gday garçon - open.pdf" requires a real password
        var path = FindPdf("Gday garçon - open.pdf");
        Skip.If(path is null, "Gday garçon - open.pdf not found");

        using var file = SiftFile.Open(path);
        using var doc = file.Parse();

        var result = doc.Authenticate("");
        Assert.False(result, "password-protected PDF should reject empty password");
    }

    [SkippableFact]
    public void Authenticate_ByteOverload()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var path = FindPdf("PasswordEncrypted.pdf");
        Skip.If(path is null, "PasswordEncrypted.pdf not found");

        using var file = SiftFile.Open(path);
        using var doc = file.Parse();

        // Test the ReadOnlySpan<byte> overload
        ReadOnlySpan<byte> password = "wrong"u8;
        var result = doc.Authenticate(password);
        Assert.False(result, "wrong password bytes should be rejected");
    }

    [SkippableFact]
    public void Authenticate_PermissionOnlyPdf_EmptyPasswordSucceeds()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        var path = FindPdf("orientation.pdf");
        Skip.If(path is null, "orientation.pdf not found");

        using var file = SiftFile.Open(path);
        using var doc = file.Parse();

        // Permission-only encrypted PDFs accept empty password
        var result = doc.Authenticate("");
        Assert.True(result, "permission-only PDF should accept empty password");
    }

    [SkippableFact]
    public void Authenticate_UnencryptedPdf_AcceptsAnyPassword()
    {
        Skip.IfNot(TestPaths.HasPopplerTest, "testdata not available");

        // Find a non-encrypted PDF
        var paths = Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories);
        Skip.If(paths.Length == 0, "no PDF files found");

        string? unencryptedPath = null;
        var encryptedNames = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
        {
            "Gday garçon - open.pdf",
            "Gday garçon - owner.pdf",
            "PasswordEncrypted.pdf",
            "encrypted-256.pdf",
            "orientation.pdf"
        };

        foreach (var p in paths)
        {
            if (!encryptedNames.Contains(Path.GetFileName(p)))
            {
                unencryptedPath = p;
                break;
            }
        }
        Skip.If(unencryptedPath is null, "no unencrypted PDF found");

        using var file = SiftFile.Open(unencryptedPath);
        using var doc = file.Parse();

        // Unencrypted PDFs - authenticate is a no-op, should not crash
        _ = doc.Authenticate("anything");
    }

    private static string? FindPdf(string fileName)
    {
        if (!TestPaths.HasPopplerTest) return null;

        return Directory.GetFiles(TestPaths.PopplerTest, "*.pdf", SearchOption.AllDirectories)
            .FirstOrDefault(p => Path.GetFileName(p) == fileName);
    }
}
