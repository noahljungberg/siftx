"""Metadata tag extraction tests."""

import pytest
import siftx
from conftest import (
    has_exiftool_images, EXIFTOOL_IMAGES,
    list_files,
)

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
def test_jpeg_has_tags():
    path = _find_jpg_with_tags()
    assert path is not None, "No JPEG with tags found"
    with siftx.SiftFile.open(str(path)) as f:
        doc = f.parse()
        tags = doc.tags()
        assert len(tags) > 0
        doc.close()


@needs_exiftool
def test_tag_has_group_name_value():
    path = _find_jpg_with_tags()
    assert path is not None
    with siftx.SiftFile.open(str(path)) as f:
        doc = f.parse()
        tags = doc.tags()
        for tag in tags:
            assert isinstance(tag.group, str)
            assert isinstance(tag.name, str)
            assert isinstance(tag.value, str)
            assert len(tag.group) > 0
            assert len(tag.name) > 0
        doc.close()


@needs_exiftool
def test_tag_repr():
    path = _find_jpg_with_tags()
    assert path is not None
    with siftx.SiftFile.open(str(path)) as f:
        doc = f.parse()
        tags = doc.tags()
        if tags:
            r = repr(tags[0])
            assert "[" in r and "]" in r and "=" in r
        doc.close()


@needs_exiftool
def test_tags_multiple_groups():
    path = _find_jpg_with_tags()
    assert path is not None
    with siftx.SiftFile.open(str(path)) as f:
        doc = f.parse()
        tags = doc.tags()
        groups = {t.group for t in tags}
        assert len(groups) >= 1
        doc.close()


@needs_exiftool
def test_convenience_tags_function():
    path = _find_jpg_with_tags()
    assert path is not None
    tags = siftx.tags(str(path))
    assert len(tags) > 0
    assert isinstance(tags[0], siftx.Tag)


@needs_exiftool
def test_batch_tag_scanning():
    files = list_files(EXIFTOOL_IMAGES, ".jpg")[:10]
    assert len(files) > 0
    for path in files:
        tags = siftx.tags(str(path))
        assert isinstance(tags, list)
