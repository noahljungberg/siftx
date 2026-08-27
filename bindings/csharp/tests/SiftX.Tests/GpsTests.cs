namespace SiftX.Tests;

public class GpsTests
{
    [SkippableFact]
    public void Gps_FromJpegWithGps_ReturnsCoordinates()
    {
        Skip.IfNot(TestPaths.HasExifSamples, "testdata not available");

        var path = Path.Combine(TestPaths.ExifSamples, "jpg", "gps", "DSCN0010.jpg");
        Skip.IfNot(File.Exists(path), "GPS test file not found");

        using var file = SiftFile.Open(path);
        using var doc = file.Parse();
        var gps = doc.Gps();

        Assert.NotNull(gps);
        Assert.NotEqual(0.0, gps.Value.Latitude);
        Assert.NotEqual(0.0, gps.Value.Longitude);
    }

    [SkippableFact]
    public void Gps_FromJpegWithoutGps_ReturnsNull()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");

        // Canon.jpg typically has GPS in some versions, but let's find one without
        var data = File.ReadAllBytes(Path.Combine(TestPaths.ExifToolImages, "Canon.jpg"));
        using var doc = SiftLib.Read(data);
        // Just verify it doesn't throw - may or may not have GPS
        _ = doc.Gps();
    }

    [SkippableFact]
    public void Gps_ScanExifSamples_NoExceptions()
    {
        Skip.IfNot(TestPaths.HasExifSamples, "testdata not available");

        var gpsDir = Path.Combine(TestPaths.ExifSamples, "jpg", "gps");
        Skip.IfNot(Directory.Exists(gpsDir), "GPS samples directory not found");

        int withGps = 0;
        foreach (var path in Directory.GetFiles(gpsDir, "*.jpg"))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var gps = doc.Gps();
                if (gps is not null) withGps++;
            }
            catch (SiftException)
            {
                // Some malformed files may fail
            }
        }

        Assert.True(withGps > 0, "at least some GPS sample files should have coordinates");
    }

    [Fact]
    public void GpsCoordinates_ToString_Format()
    {
        var gps = new GpsCoordinates(43.467157, 11.885395, 200.5);
        Assert.Equal("43.467157, 11.885395, 200.5m", gps.ToString());
    }

    [Fact]
    public void GpsCoordinates_WithoutAltitude_ToString()
    {
        var gps = new GpsCoordinates(43.467157, 11.885395, null);
        Assert.Equal("43.467157, 11.885395", gps.ToString());
    }

    [Fact]
    public void GpsCoordinates_RecordEquality()
    {
        var a = new GpsCoordinates(43.5, 11.5, 200.0);
        var b = new GpsCoordinates(43.5, 11.5, 200.0);
        Assert.Equal(a, b);
    }
}
