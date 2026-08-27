"""Version API tests."""

import siftx


def test_version_returns_string():
    v = siftx.version()
    assert isinstance(v, str)
    assert len(v) > 0


def test_version_is_semver():
    v = siftx.version()
    parts = v.split(".")
    assert len(parts) == 3
    for part in parts:
        assert part.isdigit()
