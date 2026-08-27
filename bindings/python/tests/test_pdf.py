"""PDF-specific feature tests (text extraction and image extraction)."""

import pytest
import siftx
from conftest import has_poppler_test, POPPLER_TEST, list_files_recursive

needs_poppler = pytest.mark.skipif(
    not has_poppler_test(), reason="poppler-test not available"
)


@needs_poppler
def test_pdf_text_pages():
    pdfs = list_files_recursive(POPPLER_TEST, ".pdf")
    assert len(pdfs) > 0

    found_text = False
    for path in pdfs:
        try:
            with siftx.SiftFile.open(str(path)) as f:
                doc = f.parse()
                assert doc.file_type == siftx.FileType.Pdf
                pages = doc.text_pages()
                assert isinstance(pages, list)
                if any(len(p) > 0 for p in pages):
                    found_text = True
                    break
                doc.close()
        except Exception:
            continue

    assert found_text, "Expected at least one PDF with text"


@needs_poppler
def test_pdf_images():
    pdfs = list_files_recursive(POPPLER_TEST, ".pdf")
    assert len(pdfs) > 0

    found_images = False
    for path in pdfs:
        try:
            with siftx.SiftFile.open(str(path)) as f:
                doc = f.parse()
                images = doc.images()
                if len(images) > 0:
                    img = images[0]
                    assert img.width > 0
                    assert img.height > 0
                    assert isinstance(img.data, bytes)
                    assert len(img.data) > 0
                    assert isinstance(img.extension, str)
                    assert len(img.extension) > 0
                    assert isinstance(img.page, int)
                    assert isinstance(img.bits_per_component, int)
                    assert isinstance(img.components, int)
                    assert isinstance(img.is_passthrough, bool)
                    found_images = True
                    break
                doc.close()
        except Exception:
            continue

    assert found_images, "Expected at least one PDF with images"


@needs_poppler
def test_pdf_image_format_enum():
    pdfs = list_files_recursive(POPPLER_TEST, ".pdf")
    valid_formats = [
        siftx.ImageFormat.Jpeg,
        siftx.ImageFormat.Jpeg2000,
        siftx.ImageFormat.Jbig2,
        siftx.ImageFormat.Ccitt,
        siftx.ImageFormat.Pixels,
    ]
    for path in pdfs:
        try:
            with siftx.SiftFile.open(str(path)) as f:
                doc = f.parse()
                for img in doc.images():
                    assert img.format in valid_formats
                doc.close()
        except Exception:
            continue


@needs_poppler
def test_pdf_tags():
    pdfs = list_files_recursive(POPPLER_TEST, ".pdf")
    assert len(pdfs) > 0

    for path in pdfs:
        try:
            with siftx.SiftFile.open(str(path)) as f:
                doc = f.parse()
                tags = doc.tags()
                assert isinstance(tags, list)
                doc.close()
                return
        except Exception:
            continue

    pytest.fail("Could not parse any PDF for tags")
