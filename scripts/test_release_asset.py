"""Exercise the release archive and checksum contract without GitHub Actions."""

import importlib.util
from pathlib import Path
import tarfile
import tempfile
import tomllib
import unittest


SCRIPT = Path(__file__).with_name("release_asset.py")
SPEC = importlib.util.spec_from_file_location("release_asset", SCRIPT)
release_asset = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release_asset)
with (SCRIPT.parents[1] / "Cargo.toml").open("rb") as manifest:
    TAG = f"v{tomllib.load(manifest)['package']['version']}"


class ReleaseAssetTests(unittest.TestCase):
    def test_all_targets_package_and_verify(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            binary = root / "mxo"
            binary.write_bytes(b"sample executable")
            binary.chmod(0o755)
            output = root / "dist"
            for target in release_asset.TARGETS:
                release_asset.package(TAG, target, binary, output)
                with tarfile.open(output / release_asset.asset_name(TAG, target)) as archive:
                    self.assertEqual(archive.getnames(), ["mxo"])
                    member = archive.getmember("mxo")
                    self.assertTrue(member.mode & 0o111)
                    self.assertEqual(archive.extractfile(member).read(), b"sample executable")
            release_asset.verify(TAG, output)

            first = output / release_asset.asset_name(TAG, release_asset.TARGETS[0])
            first.write_bytes(b"corrupted")
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                release_asset.verify(TAG, output)

    def test_rejects_missing_asset_and_mismatched_tag(self):
        with tempfile.TemporaryDirectory() as temp:
            with self.assertRaisesRegex(ValueError, "missing="):
                release_asset.verify(TAG, Path(temp))
        with self.assertRaisesRegex(ValueError, "does not match"):
            release_asset.validate_tag("v9999.0.0")
        with self.assertRaisesRegex(ValueError, "invalid release tag"):
            release_asset.validate_tag(f"{TAG}-rc1")


if __name__ == "__main__":
    unittest.main()
