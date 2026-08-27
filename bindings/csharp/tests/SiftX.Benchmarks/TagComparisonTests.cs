using MetadataExtractor;
using MetadataExtractor.Formats.Exif;
using MeGpsDirectory = MetadataExtractor.Formats.Exif.GpsDirectory;
using Xunit.Abstractions;

namespace SiftX.Benchmarks;

/// <summary>
/// Compare tag extraction results between Sift and MetadataExtractor
/// on the same files. Reports coverage differences, value mismatches,
/// and tags unique to each library.
/// </summary>
public class TagComparisonTests(ITestOutputHelper output)
{
    // -----------------------------------------------------------------------
    // Canon.jpg - detailed tag-by-tag comparison
    // -----------------------------------------------------------------------

    [SkippableFact]
    public void Compare_Canon_TagCounts()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");
        var path = Path.Combine(TestPaths.ExifToolImages, "Canon.jpg");

        // Sift
        using var file = SiftFile.Open(path);
        using var doc = file.Parse();
        var siftTags = doc.Tags();

        // MetadataExtractor
        var meDirs = ImageMetadataReader.ReadMetadata(path);
        var meTags = meDirs.SelectMany(d => d.Tags).ToList();

        output.WriteLine($"Sift: {siftTags.Length} tags across {siftTags.Select(t => t.Group).Distinct().Count()} groups");
        output.WriteLine($"MetadataExtractor: {meTags.Count} tags across {meDirs.Count} directories");
        output.WriteLine("");

        // Group breakdown
        output.WriteLine("=== Sift groups ===");
        foreach (var g in siftTags.GroupBy(t => t.Group).OrderByDescending(g => g.Count()))
            output.WriteLine($"  {g.Key}: {g.Count()} tags");

        output.WriteLine("");
        output.WriteLine("=== MetadataExtractor directories ===");
        foreach (var d in meDirs.Where(d => d.TagCount > 0))
            output.WriteLine($"  {d.Name}: {d.TagCount} tags");
    }

    [SkippableFact]
    public void Compare_Canon_CoreExifValues()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");
        var path = Path.Combine(TestPaths.ExifToolImages, "Canon.jpg");

        // Sift
        using var file = SiftFile.Open(path);
        using var doc = file.Parse();

        // MetadataExtractor
        var meDirs = ImageMetadataReader.ReadMetadata(path);
        var ifd0 = meDirs.OfType<ExifIfd0Directory>().FirstOrDefault();
        var exifSub = meDirs.OfType<ExifSubIfdDirectory>().FirstOrDefault();

        output.WriteLine("Tag                  | Sift                          | MetadataExtractor");
        output.WriteLine("---------------------|-------------------------------|---------------------------");

        CompareTag("Make", doc.Exif.Make, ifd0?.GetString(ExifDirectoryBase.TagMake));
        CompareTag("Model", doc.Exif.Model, ifd0?.GetString(ExifDirectoryBase.TagModel));
        CompareTag("Software", doc.Exif.Software, ifd0?.GetString(ExifDirectoryBase.TagSoftware));
        CompareTag("DateTime", doc.Exif.DateTime, ifd0?.GetString(ExifDirectoryBase.TagDateTime));
        CompareTag("Copyright", doc.Exif.Copyright, ifd0?.GetString(ExifDirectoryBase.TagCopyright));
        CompareTag("Artist", doc.Exif.Artist, ifd0?.GetString(ExifDirectoryBase.TagArtist));

        CompareTag("ExposureTime", doc.Exif.ExposureTime?.ToString(), exifSub?.GetString(ExifDirectoryBase.TagExposureTime));
        CompareTag("FNumber", doc.Exif.FNumber?.ToString("F1"), exifSub?.GetString(ExifDirectoryBase.TagFNumber));
        CompareTag("ISO", doc.Exif.Iso?.ToString(), exifSub?.GetString(ExifDirectoryBase.TagIsoEquivalent));
        CompareTag("FocalLength", doc.Exif.FocalLength?.ToString("F1"), exifSub?.GetString(ExifDirectoryBase.TagFocalLength));
        CompareTag("ExposureProgram", doc.Exif.ExposureProgram, exifSub?.GetDescription(ExifDirectoryBase.TagExposureProgram));
        CompareTag("MeteringMode", doc.Exif.MeteringMode, exifSub?.GetDescription(ExifDirectoryBase.TagMeteringMode));
        CompareTag("Flash", doc.Exif.Flash, exifSub?.GetDescription(ExifDirectoryBase.TagFlash));
        CompareTag("WhiteBalance", doc.Exif.WhiteBalance, exifSub?.GetDescription(ExifDirectoryBase.TagWhiteBalance));
        CompareTag("ColorSpace", doc.Exif.ColorSpace, exifSub?.GetDescription(ExifDirectoryBase.TagColorSpace));
        CompareTag("Orientation", doc.Exif.Orientation?.ToString(), ifd0?.GetDescription(ExifDirectoryBase.TagOrientation));
        CompareTag("ImageWidth", doc.Exif.ImageWidth?.ToString(), ifd0?.GetString(ExifDirectoryBase.TagImageWidth));
        CompareTag("ImageHeight", doc.Exif.ImageHeight?.ToString(), ifd0?.GetString(ExifDirectoryBase.TagImageHeight));
        CompareTag("DateTimeOriginal", doc.Exif.DateTimeOriginal, exifSub?.GetString(ExifDirectoryBase.TagDateTimeOriginal));
        CompareTag("LensModel", doc.Exif.LensModel, exifSub?.GetString(ExifDirectoryBase.TagLensModel));
    }

    // -----------------------------------------------------------------------
    // Batch - compare tag counts across all ExifTool images
    // -----------------------------------------------------------------------

    [SkippableFact]
    public void Compare_Batch_TagCounts()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");
        var files = System.IO.Directory.GetFiles(TestPaths.ExifToolImages, "*.jpg");
        Skip.If(files.Length == 0, "no JPEG files");

        int siftTotal = 0, meTotal = 0;
        int siftMore = 0, meMore = 0, equal = 0;

        output.WriteLine("File                         | Sift  | ME    | Winner");
        output.WriteLine("-----------------------------|-------|-------|-------");

        foreach (var path in files.OrderBy(f => f))
        {
            int siftCount = 0, meCount = 0;

            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                siftCount = doc.Tags().Length;
            }
            catch (SiftException) { }

            try
            {
                var dirs = ImageMetadataReader.ReadMetadata(path);
                meCount = dirs.Sum(d => d.TagCount);
            }
            catch { }

            siftTotal += siftCount;
            meTotal += meCount;
            var winner = siftCount > meCount ? "Sift" : siftCount < meCount ? "ME" : "Tie";
            if (siftCount > meCount) siftMore++;
            else if (meCount > siftCount) meMore++;
            else equal++;

            output.WriteLine($"{Path.GetFileName(path),-28} | {siftCount,5} | {meCount,5} | {winner}");
        }

        output.WriteLine("");
        output.WriteLine($"TOTALS: Sift={siftTotal}, MetadataExtractor={meTotal}");
        output.WriteLine($"Sift wins: {siftMore}, ME wins: {meMore}, Ties: {equal}");
    }

    // -----------------------------------------------------------------------
    // GPS comparison
    // -----------------------------------------------------------------------

    [SkippableFact]
    public void Compare_Gps_ExifSamples()
    {
        Skip.IfNot(TestPaths.HasExifSamples, "testdata not available");
        var gpsDir = Path.Combine(TestPaths.ExifSamples, "jpg", "gps");
        Skip.IfNot(System.IO.Directory.Exists(gpsDir), "GPS samples not found");
        var files = System.IO.Directory.GetFiles(gpsDir, "*.jpg");

        int siftGps = 0, meGps = 0, both = 0;

        output.WriteLine("File                    | Sift Lat/Lon                  | ME Lat/Lon");
        output.WriteLine("------------------------|-------------------------------|----------------------------");

        foreach (var path in files.Take(20))
        {
            string siftStr = "-", meStr = "-";

            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var gps = doc.Gps();
                if (gps is { } g)
                {
                    siftStr = $"{g.Latitude:F6}, {g.Longitude:F6}";
                    siftGps++;
                }
            }
            catch (SiftException) { }

            try
            {
                var dirs = ImageMetadataReader.ReadMetadata(path);
                var gpsDirectory = dirs.OfType<MeGpsDirectory>().FirstOrDefault();
                var loc = gpsDirectory?.GetGeoLocation();
                if (loc is { } geo)
                {
                    meStr = $"{geo.Latitude:F6}, {geo.Longitude:F6}";
                    meGps++;
                }
            }
            catch { }

            if (siftStr != "-" && meStr != "-") both++;
            output.WriteLine($"{Path.GetFileName(path),-23} | {siftStr,-29} | {meStr}");
        }

        output.WriteLine("");
        output.WriteLine($"GPS found - Sift: {siftGps}, MetadataExtractor: {meGps}, Both: {both}");
    }

    // -----------------------------------------------------------------------
    // MakerNotes comparison
    // -----------------------------------------------------------------------

    [SkippableFact]
    public void Compare_MakerNotes_Coverage()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");
        var files = System.IO.Directory.GetFiles(TestPaths.ExifToolImages, "*.jpg");

        output.WriteLine("File                         | Sift MN | ME MN dirs | ME MN tags");
        output.WriteLine("-----------------------------|---------|------------|----------");

        foreach (var path in files.OrderBy(f => f))
        {
            int siftMn = 0, meMnDirs = 0, meMnTags = 0;

            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                siftMn = doc.MakerNotes.Count;
            }
            catch (SiftException) { }

            try
            {
                var dirs = ImageMetadataReader.ReadMetadata(path);
                var mnDirs = dirs.Where(d => d.GetType().Namespace?.Contains("Makernotes") == true).ToList();
                meMnDirs = mnDirs.Count;
                meMnTags = mnDirs.Sum(d => d.TagCount);
            }
            catch { }

            if (siftMn > 0 || meMnTags > 0)
                output.WriteLine($"{Path.GetFileName(path),-28} | {siftMn,7} | {meMnDirs,10} | {meMnTags,9}");
        }
    }

    // -----------------------------------------------------------------------
    // Typed value comparison - EXIF numerics
    // -----------------------------------------------------------------------

    [SkippableFact]
    public void Compare_TypedValues_ExposureData()
    {
        Skip.IfNot(TestPaths.HasExifToolImages, "testdata not available");
        var files = System.IO.Directory.GetFiles(TestPaths.ExifToolImages, "*.jpg");

        output.WriteLine("File                    | Tag          | Sift (typed)       | ME (raw)");
        output.WriteLine("------------------------|--------------|--------------------|-----------------");

        foreach (var path in files.Take(15))
        {
            try
            {
                using var file = SiftFile.Open(path);
                using var doc = file.Parse();
                var meDirs = ImageMetadataReader.ReadMetadata(path);
                var exifSub = meDirs.OfType<ExifSubIfdDirectory>().FirstOrDefault();

                var name = Path.GetFileName(path);

                if (doc.Exif.Iso is int iso)
                {
                    int meIso = 0;
                    exifSub?.TryGetInt32(ExifDirectoryBase.TagIsoEquivalent, out meIso);
                    output.WriteLine($"{name,-23} | ISO          | {iso,-18} | {meIso}");
                }

                if (doc.Exif.FNumber is double fn)
                {
                    var meRat = exifSub?.GetRational(ExifDirectoryBase.TagFNumber);
                    var meFn = meRat.HasValue ? (double)meRat.Value.Numerator / meRat.Value.Denominator : 0;
                    output.WriteLine($"{name,-23} | FNumber      | {fn,-18:F2} | {meFn:F2}");
                }

                if (doc.Exif.ExposureTime is var (num, den))
                {
                    var meRat = exifSub?.GetRational(ExifDirectoryBase.TagExposureTime);
                    output.WriteLine($"{name,-23} | ExposureTime | {num}/{den,-15} | {meRat}");
                }

                if (doc.Exif.FocalLength is double fl)
                {
                    var meRat = exifSub?.GetRational(ExifDirectoryBase.TagFocalLength);
                    var meFl = meRat.HasValue ? (double)meRat.Value.Numerator / meRat.Value.Denominator : 0;
                    output.WriteLine($"{name,-23} | FocalLength  | {fl,-18:F1} | {meFl:F1}");
                }
            }
            catch { }
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    private void CompareTag(string name, string? siftVal, string? meVal)
    {
        var s = siftVal ?? "-";
        var m = meVal ?? "-";
        // Truncate for display
        if (s.Length > 29) s = s[..26] + "...";
        if (m.Length > 25) m = m[..22] + "...";
        output.WriteLine($"{name,-20} | {s,-29} | {m}");
    }
}
