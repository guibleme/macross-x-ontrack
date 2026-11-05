import importlib.util
import json
import tempfile
import unittest
import zipfile
from pathlib import Path

spec = importlib.util.spec_from_file_location("neutral_audit", Path(__file__).parents[1] / "scripts/audit-neutral.py")
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class ReleaseAudit(unittest.TestCase):
    def test_reports_no_sensitive_matched_values(self):
        sensitive = "gh" + "p_" + "a" * 30
        findings = audit.inspect_bytes("bundle.js", sensitive.encode())
        self.assertEqual(findings, [{"path": "bundle.js", "category": "private-credential"}])
        self.assertNotIn(sensitive, json.dumps(findings))

    def test_scans_archived_artifacts_without_extraction(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "release.zip"
            with zipfile.ZipFile(path, "w") as archive:
                archive.writestr("dist/app.js.map", "/Users/" + "person/private/source.ts")
            result = audit.audit(root, [path.name])
            self.assertFalse(result["passed"])
            self.assertEqual(result["findings"][0]["category"], "private-path")

    def test_neutral_artifact_passes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "app.js").write_text('const title = "Workfront";')
            self.assertTrue(audit.audit(root, ["app.js"])["passed"])

    def test_symlinks_require_review(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "link").symlink_to("/outside")
            self.assertEqual(audit.audit(root, ["link"])["findings"][0]["category"], "symlink-review")


if __name__ == "__main__":
    unittest.main()
