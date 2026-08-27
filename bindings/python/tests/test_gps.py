"""GPS coordinate extraction tests."""

import pytest
import siftx
from conftest import (
    has_exif_samples, EXIF_SAMPLES,
    has_exiftool_images, EXIFTOOL_IMAGES,
    find_first, list_files_recursive,
)

needs_exif_samples = pytest.mark.skipif(
    not has_exif_samples(), reason="exif-samples not available"
)
needs_exiftool = pytest.mark.skipif(
    not has_exiftool_images(), reason="exiftool-images not available"
)


@needs_exif_samples
def test_gps_from_sample():
    gps_dir = EXIF_SAMPLES / "jpg" / "gps"
    files = list_files_recursive(gps_dir, ".jpg")
    assert len(files) > 0

    found_gps = False
    for path in files:
        with siftx.SiftFile.open(str(path)) as f:
            doc = f.parse()
            gps = doc.gps()
            if gps is not None:
                assert isinstance(gps.latitude, float)
                assert isinstance(gps.longitude, float)
                assert -90 <= gps.latitude <= 90
                assert -180 <= gps.longitude <= 180
                found_gps = True
            doc.close()

    assert found_gps, "Expected at least one GPS sample"


@needs_exif_samples
def test_gps_repr():
    gps_dir = EXIF_SAMPLES / "jpg" / "gps"
    files = list_files_recursive(gps_dir, ".jpg")
    for path in files:
        with siftx.SiftFile.open(str(path)) as f:
            doc = f.parse()
            gps = doc.gps()
            if gps is not None:
                r = repr(gps)
                assert "." in r  # Should contain decimal coordinates
                break
            doc.close()


@needs_exiftool
def test_no_gps_returns_none():
    """Most ExifTool test images don't have GPS data."""
    path = find_first(EXIFTOOL_IMAGES, ".png")
    if path is None:
        pytest.skip("No PNG test files")
    with siftx.SiftFile.open(str(path)) as f:
        doc = f.parse()
        gps = doc.gps()
        assert gps is None
        doc.close()


@needs_exif_samples
def test_gps_altitude():
    gps_dir = EXIF_SAMPLES / "jpg" / "gps"
    files = list_files_recursive(gps_dir, ".jpg")
    for path in files:
        with siftx.SiftFile.open(str(path)) as f:
            doc = f.parse()
            gps = doc.gps()
            if gps is not None and gps.altitude is not None:
                assert isinstance(gps.altitude, float)
                return
            doc.close()
    # It's ok if no files have altitude
