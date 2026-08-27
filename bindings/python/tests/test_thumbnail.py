"""EXIF thumbnail extraction tests."""

import pytest
import siftx
from conftest import (
    has_exiftool_images, EXIFTOOL_IMAGES,
    find_first, list_files,
)

needs_exiftool = pytest.mark.skipif(
    not has_exiftool_images(), reason="exiftool-images not available"
)

JPEG_SOI = b"\xff\xd8"
JPEG_EOI = b"\xff\xd9"


@needs_exiftool
def test_jpeg_thumbnail():
    files = list_files(EXIFTOOL_IMAGES, ".jpg")
    found_thumb = False
    for path in files:
        with siftx.SiftFile.open(str(path)) as f:
            doc = f.parse()
            thumb = doc.thumbnail()
            if thumb is not None:
                assert isinstance(thumb, bytes)
                assert len(thumb) > 0
                assert thumb[:2] == JPEG_SOI
                assert thumb[-2:] == JPEG_EOI
                found_thumb = True
                break
            doc.close()

    assert found_thumb, "Expected at least one JPEG with a thumbnail"


@needs_exiftool
def test_no_thumbnail_returns_none():
    path = find_first(EXIFTOOL_IMAGES, ".png")
    if path is None:
        pytest.skip("No PNG test files")
    with siftx.SiftFile.open(str(path)) as f:
        doc = f.parse()
        thumb = doc.thumbnail()
        assert thumb is None
        doc.close()


@needs_exiftool
def test_thumbnail_batch_scan():
    """Scan multiple JPEGs for thumbnails without crashing."""
    files = list_files(EXIFTOOL_IMAGES, ".jpg")[:20]
    thumb_count = 0
    for path in files:
        with siftx.SiftFile.open(str(path)) as f:
            doc = f.parse()
            thumb = doc.thumbnail()
            if thumb is not None:
                assert isinstance(thumb, bytes)
                assert len(thumb) > 100
                thumb_count += 1
            doc.close()
    # Just ensure no crashes; some files may not have thumbnails
