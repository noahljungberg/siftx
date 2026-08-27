"""Shared fixtures and helpers for siftx tests."""

import os
from pathlib import Path


def find_repo_root() -> Path:
    """Walk up from this file to find the repo root (contains Cargo.toml)."""
    p = Path(__file__).resolve()
    while p != p.parent:
        if (p / "Cargo.toml").exists() and (p / "src").is_dir():
            # Make sure it's the workspace root, not bindings/python
            if (p / "testdata").is_dir() or (p / "docs").is_dir():
                return p
        p = p.parent
    raise RuntimeError("Could not find repo root")


REPO_ROOT = find_repo_root()
EXIFTOOL_IMAGES = REPO_ROOT / "testdata" / "exiftool-images"
EXIF_SAMPLES = REPO_ROOT / "testdata" / "exif-samples"
POPPLER_TEST = REPO_ROOT / "testdata" / "poppler-test"


def has_exiftool_images() -> bool:
    return EXIFTOOL_IMAGES.is_dir()


def has_exif_samples() -> bool:
    return EXIF_SAMPLES.is_dir()


def has_poppler_test() -> bool:
    return POPPLER_TEST.is_dir()


def find_first(directory: Path, ext: str) -> Path | None:
    """Find the first file with the given extension in a directory."""
    for f in sorted(directory.iterdir()):
        if f.is_file() and f.suffix.lower() == ext.lower():
            return f
    return None


def list_files(directory: Path, ext: str) -> list[Path]:
    """List files with the given extension in a directory (non-recursive)."""
    if not directory.is_dir():
        return []
    return sorted(
        f for f in directory.iterdir()
        if f.is_file() and f.suffix.lower() == ext.lower()
    )


def list_files_recursive(directory: Path, ext: str) -> list[Path]:
    """List files with the given extension recursively."""
    if not directory.is_dir():
        return []
    return sorted(directory.rglob(f"*{ext}"))
