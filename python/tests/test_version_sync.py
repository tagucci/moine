import importlib.util
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts" / "check-version-sync.py"


def load_version_checker():
    specification = importlib.util.spec_from_file_location("check_version_sync", SCRIPT)
    assert specification is not None
    assert specification.loader is not None
    module = importlib.util.module_from_spec(specification)
    previous = sys.dont_write_bytecode
    sys.dont_write_bytecode = True
    try:
        specification.loader.exec_module(module)
    finally:
        sys.dont_write_bytecode = previous
    return module


def write_project(
    root,
    *,
    core_version="0.2.2",
    dependency_version="0.2.2",
    lock_version="0.2.2",
):
    (root / "pyproject.toml").write_text(
        '[project]\nname = "moine"\nversion = "0.2.2"\n',
        encoding="utf-8",
    )
    core_dir = root / "crates" / "moine-core"
    core_dir.mkdir(parents=True)
    (core_dir / "Cargo.toml").write_text(
        f'[package]\nname = "moine-core"\nversion = "{core_version}"\n',
        encoding="utf-8",
    )
    ja_dir = root / "crates" / "moine-ja"
    ja_dir.mkdir(parents=True)
    (ja_dir / "Cargo.toml").write_text(
        "\n".join(
            [
                "[package]",
                'name = "moine-ja"',
                'version = "0.2.2"',
                "",
                "[dependencies]",
                f'moine-core = {{ version = "{dependency_version}", path = "../moine-core" }}',
                "",
            ]
        ),
        encoding="utf-8",
    )
    (root / "Cargo.lock").write_text(
        "\n".join(
            [
                "version = 4",
                "",
                "[[package]]",
                'name = "moine-core"',
                f'version = "{lock_version}"',
                "",
                "[[package]]",
                'name = "moine-ja"',
                'version = "0.2.2"',
                "",
            ]
        ),
        encoding="utf-8",
    )


def configure_test_project(module, monkeypatch, root):
    monkeypatch.setattr(module, "ROOT", root)
    monkeypatch.setattr(module, "INTERNAL_PACKAGES", {"moine-core", "moine-ja"})


def test_version_checker_accepts_matching_packages_dependencies_and_tag(tmp_path, monkeypatch):
    module = load_version_checker()
    write_project(tmp_path)
    configure_test_project(module, monkeypatch, tmp_path)

    assert module.check_versions("v0.2.2") == []


def test_version_checker_reports_package_dependency_and_tag_mismatches(tmp_path, monkeypatch):
    module = load_version_checker()
    write_project(
        tmp_path,
        core_version="0.2.1",
        dependency_version="0.2.1",
        lock_version="0.2.1",
    )
    configure_test_project(module, monkeypatch, tmp_path)

    failures = module.check_versions("v0.2.3")

    assert any("package 'moine-core' has version '0.2.1'" in failure for failure in failures)
    assert any(
        "internal dependency 'moine-core' has version '0.2.1'" in failure for failure in failures
    )
    assert any(
        "Cargo.lock: package 'moine-core' has version '0.2.1'" in failure for failure in failures
    )
    assert any("release tag 'v0.2.3'" in failure for failure in failures)
