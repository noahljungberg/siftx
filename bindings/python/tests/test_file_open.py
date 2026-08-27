"""File opening and type detection tests."""

import pytest
import siftx
from conftest import has_exiftool_images, EXIFTOOL_IMAGES, find_first

needs_exiftool = pytest.mark.skipif(
    not has_exiftool_images(), reason="exiftool-images not available"
)


@needs_exiftool
def test_open_jpeg():
    path = find_first(EXIFTOOL_IMAGES, ".jpg")
    assert path is not None
    f = siftx.SiftFile.open(str(path))
    assert f.file_type == siftx.FileType.Jpeg
    f.close()


@needs_exiftool
def test_open_and_parse():
    path = find_first(EXIFTOOL_IMAGES, ".jpg")
    assert path is not None
    f = siftx.SiftFile.open(str(path))
    doc = f.parse()
    assert doc.file_type == siftx.FileType.Jpeg
    doc.close()


@needs_exiftool
def test_context_manager():
    path = find_first(EXIFTOOL_IMAGES, ".jpg")
    assert path is not None
    with siftx.SiftFile.open(str(path)) as f:
        doc = f.parse()
        assert doc.file_type == siftx.FileType.Jpeg
        doc.close()


@needs_exiftool
def test_closed_file_raises():
    path = find_first(EXIFTOOL_IMAGES, ".jpg")
    assert path is not None
    f = siftx.SiftFile.open(str(path))
    f.close()
    with pytest.raises(RuntimeError, match="closed"):
        _ = f.file_type


def test_open_nonexistent_raises():
    with pytest.raises(IOError):
        siftx.SiftFile.open("/nonexistent/file.jpg")


@needs_exiftool
def test_file_type_detection():
    type_map = {
        ".jpg": siftx.FileType.Jpeg,
        ".png": siftx.FileType.Png,
        ".gif": siftx.FileType.Gif,
        ".tif": siftx.FileType.Tiff,
        ".webp": siftx.FileType.WebP,
    }
    for ext, expected_type in type_map.items():
        path = find_first(EXIFTOOL_IMAGES, ext)
        if path is not None:
            with siftx.SiftFile.open(str(path)) as f:
                assert f.file_type == expected_type, f"Expected {expected_type} for {ext}"
