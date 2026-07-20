#!/usr/bin/env python3
"""Check that all mòine package and internal dependency versions agree."""

from __future__ import annotations

import argparse
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INTERNAL_PACKAGES = {
    "moine",
    "moine-cli",
    "moine-core",
    "moine-ja",
    "moine-python",
    "moine-wasm",
    "moine-zh",
}
SECTION_RE = re.compile(r"^\[([^]]+)]$")
KEY_ASSIGNMENT_RE = re.compile(r"^([A-Za-z0-9_-]+)\s*=")
STRING_ASSIGNMENT_RE = re.compile(r'^([A-Za-z0-9_-]+)\s*=\s*"([^"]+)"')
INLINE_VERSION_RE = re.compile(r'\bversion\s*=\s*"([^"]+)"')
PACKAGE_ARRAY_RE = re.compile(r"^\[\[package]]$")


def project_version(path: Path) -> str:
    section = ""
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.split("#", 1)[0].strip()
        section_match = SECTION_RE.fullmatch(line)
        if section_match:
            section = section_match.group(1)
            continue
        assignment = STRING_ASSIGNMENT_RE.match(line)
        if section == "project" and assignment and assignment.group(1) == "version":
            return assignment.group(2)
    raise ValueError(f"{path.relative_to(ROOT)}: missing string project.version")


def is_dependency_table(section: str) -> bool:
    return section in {"dependencies", "dev-dependencies", "build-dependencies"} or any(
        section.endswith(f".{name}")
        for name in ("dependencies", "dev-dependencies", "build-dependencies")
    )


def dependency_section_name(section: str) -> str | None:
    markers = ("dependencies.", "dev-dependencies.", "build-dependencies.")
    for marker in markers:
        marker_index = section.rfind(marker)
        if marker_index >= 0:
            return section[marker_index + len(marker) :]
    return None


def cargo_manifest_versions(path: Path) -> tuple[str, str, list[tuple[str, str, str | None]]]:
    section = ""
    package_name: str | None = None
    package_version: str | None = None
    dependencies: list[tuple[str, str, str | None]] = []

    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.split("#", 1)[0].strip()
        if not line:
            continue
        section_match = SECTION_RE.fullmatch(line)
        if section_match:
            section = section_match.group(1)
            continue

        assignment = STRING_ASSIGNMENT_RE.match(line)
        if section == "package" and assignment:
            if assignment.group(1) == "name":
                package_name = assignment.group(2)
            elif assignment.group(1) == "version":
                package_version = assignment.group(2)
            continue

        dependency_assignment = KEY_ASSIGNMENT_RE.match(line)
        if is_dependency_table(section) and dependency_assignment:
            dependency_name = dependency_assignment.group(1)
            if dependency_name in INTERNAL_PACKAGES:
                version_match = INLINE_VERSION_RE.search(line)
                version = version_match.group(1) if version_match else None
                dependencies.append((section, dependency_name, version))
            continue

        dependency_name = dependency_section_name(section)
        if dependency_name in INTERNAL_PACKAGES and assignment and assignment.group(1) == "version":
            dependencies.append((section, dependency_name, assignment.group(2)))

    relative_path = path.relative_to(ROOT)
    if package_name is None:
        raise ValueError(f"{relative_path}: missing string package.name")
    if package_version is None:
        raise ValueError(f"{relative_path}: missing string package.version")
    return package_name, package_version, dependencies


def cargo_lock_versions(path: Path) -> dict[str, str]:
    versions: dict[str, str] = {}
    package_name: str | None = None
    package_version: str | None = None

    def save_package() -> None:
        if package_name in INTERNAL_PACKAGES and package_version is not None:
            versions[package_name] = package_version

    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.split("#", 1)[0].strip()
        if PACKAGE_ARRAY_RE.fullmatch(line):
            save_package()
            package_name = None
            package_version = None
            continue
        assignment = STRING_ASSIGNMENT_RE.match(line)
        if assignment and assignment.group(1) == "name":
            package_name = assignment.group(2)
        elif assignment and assignment.group(1) == "version":
            package_version = assignment.group(2)
    save_package()
    return versions


def check_versions(release_tag: str | None) -> list[str]:
    failures: list[str] = []
    try:
        expected = project_version(ROOT / "pyproject.toml")
    except ValueError as error:
        return [str(error)]

    seen_packages: set[str] = set()
    for manifest_path in sorted(ROOT.glob("crates/*/Cargo.toml")):
        try:
            package_name, package_version, dependencies = cargo_manifest_versions(manifest_path)
        except ValueError as error:
            failures.append(str(error))
            continue
        seen_packages.add(package_name)
        if package_version != expected:
            failures.append(
                f"{manifest_path.relative_to(ROOT)}: package {package_name!r} has version "
                f"{package_version!r}, expected {expected!r}"
            )

        for table_name, dependency_name, version in dependencies:
            if version != expected:
                failures.append(
                    f"{manifest_path.relative_to(ROOT)} [{table_name}]: internal dependency "
                    f"{dependency_name!r} has version {version!r}, expected {expected!r}"
                )

    missing = INTERNAL_PACKAGES - seen_packages
    if missing:
        failures.append(f"workspace is missing internal packages: {', '.join(sorted(missing))}")

    lock_versions = cargo_lock_versions(ROOT / "Cargo.lock")
    for package_name in sorted(INTERNAL_PACKAGES):
        lock_version = lock_versions.get(package_name)
        if lock_version != expected:
            failures.append(
                f"Cargo.lock: package {package_name!r} has version {lock_version!r}, "
                f"expected {expected!r}"
            )

    if release_tag and release_tag != f"v{expected}":
        failures.append(f"release tag {release_tag!r} does not match package version 'v{expected}'")

    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--tag",
        default=os.environ.get("MOINE_RELEASE_TAG") or None,
        help="optional release tag to compare with the package version",
    )
    args = parser.parse_args()

    failures = check_versions(args.tag)
    if failures:
        for failure in failures:
            print(f"error: {failure}", file=sys.stderr)
        return 1

    version = project_version(ROOT / "pyproject.toml")
    suffix = f" and tag {args.tag}" if args.tag else ""
    print(f"package versions agree on {version}{suffix}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
