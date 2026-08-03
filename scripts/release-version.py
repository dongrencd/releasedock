#!/usr/bin/env python3
"""Synchronize the ReleaseDock application version across release surfaces.

The repository intentionally keeps generated lockfiles committed, so this
script updates only the package-owned version records and leaves third-party
dependency versions untouched.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


VERSION_PATTERN = re.compile(r"^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$")

MANIFESTS = (
    Path("crates/core/Cargo.toml"),
    Path("crates/cli/Cargo.toml"),
    Path("apps/desktop/src-tauri/Cargo.toml"),
)


def validate_version(version: str) -> str:
    """Return a normalized patch version or reject unsupported tag shapes."""

    version = version.strip()
    if not VERSION_PATTERN.fullmatch(version):
        raise ValueError(
            f"invalid version {version!r}; expected MAJOR.MINOR.PATCH without a v prefix"
        )
    return version


def replace_once(
    content: str,
    pattern: str,
    replacement: str,
    path: Path,
    expected_count: int = 1,
) -> str:
    """Replace known project-owned markers and fail closed on layout drift."""

    updated, count = re.subn(pattern, replacement, content, count=expected_count, flags=re.MULTILINE)
    if count != expected_count:
        raise ValueError(
            f"expected {expected_count} version marker(s) in {path}, found {count}"
        )
    return updated


def update_manifest(root: Path, relative_path: Path, version: str) -> str:
    path = root / relative_path
    return replace_once(
        path.read_text(encoding="utf-8"),
        r'^(version\s*=\s*")[^"]+("\s*)$',
        rf'\g<1>{version}\g<2>',
        relative_path,
    )


def update_json_version(root: Path, relative_path: Path, version: str, count: int = 1) -> str:
    path = root / relative_path
    return replace_once(
        path.read_text(encoding="utf-8"),
        r'^(\s*"version"\s*:\s*")[^"]+("\s*,?\s*)$',
        rf'\g<1>{version}\g<2>',
        relative_path,
        expected_count=count,
    )


def update_lockfile(root: Path, relative_path: Path, package_names: tuple[str, ...], version: str) -> str:
    path = root / relative_path
    content = path.read_text(encoding="utf-8")
    for package_name in package_names:
        content = replace_once(
            content,
            rf'(^name\s*=\s*"{re.escape(package_name)}"\s*\nversion\s*=\s*")[^"]+("\s*$)',
            rf'\g<1>{version}\g<2>',
            relative_path,
        )
    return content


def update_readme(root: Path, relative_path: Path, version: str) -> str:
    path = root / relative_path
    content = path.read_text(encoding="utf-8")
    content = replace_once(
        content,
        r'^(git tag v)[^\s]+$',
        rf'\g<1>{version}',
        relative_path,
    )
    return replace_once(
        content,
        r'^(git push origin v)[^\s]+$',
        rf'\g<1>{version}',
        relative_path,
    )


def build_expected_files(root: Path, version: str) -> dict[Path, str]:
    """Build the complete expected version surface without writing files."""

    version = validate_version(version)
    expected = {Path("VERSION"): f"{version}\n"}
    for relative_path in MANIFESTS:
        expected[relative_path] = update_manifest(root, relative_path, version)
    expected[Path("apps/desktop/package.json")] = update_json_version(
        root, Path("apps/desktop/package.json"), version
    )
    expected[Path("apps/desktop/package-lock.json")] = update_json_version(
        root, Path("apps/desktop/package-lock.json"), version, count=2
    )
    expected[Path("apps/desktop/src-tauri/tauri.conf.json")] = update_json_version(
        root, Path("apps/desktop/src-tauri/tauri.conf.json"), version
    )
    expected[Path("Cargo.lock")] = update_lockfile(
        root,
        Path("Cargo.lock"),
        ("releasedock-cli", "releasedock-core"),
        version,
    )
    expected[Path("apps/desktop/src-tauri/Cargo.lock")] = update_lockfile(
        root,
        Path("apps/desktop/src-tauri/Cargo.lock"),
        ("releasedock", "releasedock-cli", "releasedock-core"),
        version,
    )
    for relative_path in (Path("README.md"), Path("README_zh-CN.md")):
        expected[relative_path] = update_readme(root, relative_path, version)
    return expected


def sync_repository(root: Path, version: str) -> list[Path]:
    """Write the requested version and return files whose contents changed."""

    expected = build_expected_files(root, version)
    changed: list[Path] = []
    for relative_path, content in expected.items():
        path = root / relative_path
        current = path.read_text(encoding="utf-8") if path.exists() else None
        if current == content:
            continue
        path.write_text(content, encoding="utf-8")
        changed.append(relative_path)
    return changed


def check_repository(root: Path) -> list[Path]:
    """Return project files that differ from the canonical VERSION value."""

    version_path = root / "VERSION"
    if not version_path.exists():
        return [Path("VERSION")]
    version = validate_version(version_path.read_text(encoding="utf-8"))
    expected = build_expected_files(root, version)
    return [
        relative_path
        for relative_path, content in expected.items()
        if not (root / relative_path).exists()
        or (root / relative_path).read_text(encoding="utf-8") != content
    ]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "version",
        nargs="?",
        help="new MAJOR.MINOR.PATCH version; omit only when using --check",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify every version surface matches VERSION without writing files",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = Path(__file__).resolve().parent.parent
    try:
        if args.check:
            if args.version:
                raise ValueError("--check cannot be combined with a version argument")
            mismatches = check_repository(root)
            if mismatches:
                print("Version drift detected:")
                for relative_path in mismatches:
                    print(f"- {relative_path}")
                return 1
            print(f"Version surfaces match {root / 'VERSION'}")
            return 0
        if not args.version:
            raise ValueError("a version argument is required unless --check is used")
        changed = sync_repository(root, args.version)
        print(f"Synchronized version {validate_version(args.version)}")
        for relative_path in changed:
            print(f"- {relative_path}")
        return 0
    except (OSError, ValueError) as error:
        print(f"release-version: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
