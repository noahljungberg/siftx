namespace SiftX.Tests;

public class VersionTests
{
    [Fact]
    public void Version_ReturnsValidString()
    {
        var version = SiftLib.Version;
        Assert.NotNull(version);
        Assert.NotEmpty(version);
        Assert.Equal("0.1.0", version);
    }
}
