"""Buffer-based document parsing tests."""

import pytest
import siftx
from conftest import has_exiftool_images, EXIFTOOL_IMAGES, list_files

needs_exiftool = pytest.mark.skipif(
    not has_exiftool_images(), reason="exiftool-images not available"
)


def _find_jpg_with_tags():
    """Find a JPEG that actually has metadata tags."""
    for path in list_files(EXIFTOOL_IMAGES, ".jpg"):
        tags = siftx.tags(str(path))
        if len(tags) > 0:
            return path
    return None


@needs_exiftool
def test_read_from_bytes():
    path = _find_jpg_with_tags()
    assert path is not None
    data = path.read_bytes()
    doc = siftx.read(data)
    assert doc.file_type == siftx.FileType.Jpeg
    doc.close()


@needs_exiftool
def test_read_tags_match_file():
    path = _find_jpg_with_tags()
    assert path is not None

    # Tags via file
    with siftx.SiftFile.open(str(path)) as f:
        doc = f.parse()
        file_tags = doc.tags()
        doc.close()

    # Tags via buffer
    data = path.read_bytes()
    doc = siftx.read(data)
    buf_tags = doc.tags()
    doc.close()

    assert len(file_tags) == len(buf_tags)
    for ft, bt in zip(file_tags, buf_tags):
        assert ft.group == bt.group
        assert ft.name == bt.name
        assert ft.value == bt.value


@needs_exiftool
def test_read_buffer_is_copied():
    """Verify that modifying the original buffer doesn't affect the document."""
    path = _find_jpg_with_tags()
    assert path is not None
    data = bytearray(path.read_bytes())
    doc = siftx.read(bytes(data))
    # Mutate original - should not affect doc
    data[0] = 0
    tags = doc.tags()
    assert len(tags) > 0
    doc.close()


def test_read_empty_bytes_returns_unknown():
    doc = siftx.read(b"")
    assert doc.file_type == siftx.FileType.Unknown
    doc.close()


def test_read_garbage_returns_unknown():
    doc = siftx.read(b"not a valid file format at all")
    assert doc.file_type == siftx.FileType.Unknown
    assert len(doc.tags()) == 0
    doc.close()
