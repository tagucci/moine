from importlib.metadata import version

import moine
from moine import _moine


def test_version_matches_package_metadata() -> None:
    assert moine.__version__ == version("moine")


def test_native_extension_exposes_version() -> None:
    assert _moine.__version__ == moine.__version__
