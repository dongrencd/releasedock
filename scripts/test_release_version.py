import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).with_name("release-version.py")
SPEC = importlib.util.spec_from_file_location("release_version", SCRIPT_PATH)
assert SPEC and SPEC.loader
release_version = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release_version)


FIXTURES = {
    "crates/core/Cargo.toml": '[package]\nname = "releasedock-core"\nversion = "0.2.13"\n',
    "crates/cli/Cargo.toml": '[package]\nname = "releasedock-cli"\nversion = "0.2.13"\n',
    "apps/desktop/src-tauri/Cargo.toml": '[package]\nname = "releasedock"\nversion = "0.2.13"\n',
    "apps/desktop/package.json": '{\n  "name": "releasedock-desktop",\n  "version": "0.2.13"\n}\n',
    "apps/desktop/package-lock.json": '{\n  "version": "0.2.13",\n  "packages": {\n    "": {\n      "version": "0.2.13"\n    }\n  }\n}\n',
    "apps/desktop/src-tauri/tauri.conf.json": '{\n  "version": "0.2.13"\n}\n',
    "README.md": "git tag v0.2.13\ngit push origin v0.2.13\n",
    "README_zh-CN.md": "git tag v0.2.13\ngit push origin v0.2.13\n",
    "Cargo.lock": (
        'name = "releasedock-cli"\nversion = "0.2.13"\n\n'
        'name = "releasedock-core"\nversion = "0.2.13"\n'
    ),
    "apps/desktop/src-tauri/Cargo.lock": (
        'name = "releasedock"\nversion = "0.2.13"\n\n'
        'name = "releasedock-cli"\nversion = "0.2.13"\n\n'
        'name = "releasedock-core"\nversion = "0.2.13"\n'
    ),
}


class ReleaseVersionTests(unittest.TestCase):
    def make_repository(self) -> Path:
        root = Path(tempfile.mkdtemp())
        for relative_path, content in FIXTURES.items():
            path = root / relative_path
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        return root

    def test_sync_updates_all_project_version_surfaces(self) -> None:
        root = self.make_repository()

        release_version.sync_repository(root, "0.2.14")

        self.assertEqual((root / "VERSION").read_text(encoding="utf-8"), "0.2.14\n")
        self.assertIn('version = "0.2.14"', (root / "Cargo.lock").read_text())
        self.assertIn('"version": "0.2.14"', (root / "apps/desktop/package-lock.json").read_text())
        self.assertIn("git tag v0.2.14", (root / "README.md").read_text())

    def test_sync_is_idempotent(self) -> None:
        root = self.make_repository()

        release_version.sync_repository(root, "0.2.14")
        first_snapshot = {
            path: (root / path).read_text(encoding="utf-8")
            for path in [*FIXTURES, "VERSION"]
        }
        release_version.sync_repository(root, "0.2.14")

        second_snapshot = {
            path: (root / path).read_text(encoding="utf-8")
            for path in [*FIXTURES, "VERSION"]
        }
        self.assertEqual(first_snapshot, second_snapshot)

    def test_invalid_versions_are_rejected(self) -> None:
        root = self.make_repository()

        for invalid_version in ["2.14", "v0.2.14", "0.2.14-beta", "01.2.3"]:
            with self.subTest(invalid_version=invalid_version):
                with self.assertRaises(ValueError):
                    release_version.sync_repository(root, invalid_version)


if __name__ == "__main__":
    unittest.main()
