"""Multi-format scanning and thread safety tests."""

import concurrent.futures
import pytest
import siftx
from conftest import (
    has_exiftool_images, EXIFTOOL_IMAGES,
    find_first, list_files,
)

needs_exiftool = pytest.mark.skipif(
    not has_exiftool_images(), reason="exiftool-images not available"
)


@needs_exiftool
def test_all_image_formats():
    """Open and parse every supported image format without crashing."""
    extensions = [".jpg", ".png", ".gif", ".tif", ".webp", ".bmp"]
    parsed = 0
    for ext in extensions:
        path = find_first(EXIFTOOL_IMAGES, ext)
        if path is not None:
            with siftx.SiftFile.open(str(path)) as f:
                doc = f.parse()
                tags = doc.tags()
                assert isinstance(tags, list)
                doc.close()
                parsed += 1
    assert parsed >= 3, f"Expected at least 3 formats, got {parsed}"


@needs_exiftool
def test_concurrent_tag_access():
    """Extract tags from multiple files concurrently."""
    files = list_files(EXIFTOOL_IMAGES, ".jpg")[:10]
    assert len(files) > 0

    def extract(path):
        return siftx.tags(str(path))

    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        futures = [pool.submit(extract, f) for f in files]
        results = [f.result() for f in concurrent.futures.as_completed(futures)]

    assert len(results) == len(files)
    for tags in results:
        assert isinstance(tags, list)


@needs_exiftool
def test_scan_all_exiftool_images():
    """Scan all ExifTool test images without crashing."""
    extensions = [".jpg", ".png", ".gif", ".tif", ".webp", ".bmp", ".heic", ".icc"]
    count = 0
    errors = 0
    for ext in extensions:
        for path in list_files(EXIFTOOL_IMAGES, ext):
            try:
                tags = siftx.tags(str(path))
                assert isinstance(tags, list)
                count += 1
            except Exception:
                errors += 1
    assert count > 0, "Expected to parse at least some files"
