"""Fixed metrics scope, changed-only writes, and atomic replacement regressions."""
import hashlib
import importlib.util
from pathlib import Path
import stat
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("metrics_assets", Path(__file__).with_name("remote_assets_batch.py"))
BATCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BATCH)


def asset(path, body):
    return {"path": path, "body": body, "sha256": hashlib.sha256(body.encode()).hexdigest()}


def request(home):
    return {"v": 1, "home": str(home), "bot_id": "10001", "assets": [
        asset("ncd-ob11-stats.cjs", "probe-fixture"),
        asset("10001/nodes.json", "{}"),
        asset("snowluma-shared/10001.json", '{"enabled":false}'),
    ]}


class MetricsAssetsTests(unittest.TestCase):
    def test_unchanged_assets_do_not_rewrite_and_preferences_update_only_registration(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            value = request(home)
            self.assertEqual(BATCH.synchronize(value)["changed"], 3)
            root = home / "ncd-watch" / "metrics"
            before = {item["path"]: (root / item["path"]).stat().st_mtime_ns for item in value["assets"]}
            self.assertEqual(BATCH.synchronize(value)["changed"], 0)
            self.assertEqual(before, {name: (root / name).stat().st_mtime_ns for name in before})
            value["assets"][2] = asset("snowluma-shared/10001.json", '{"enabled":true}')
            self.assertEqual(BATCH.synchronize(value)["changed"], 1)
            self.assertEqual((root / "snowluma-shared/10001.json").read_text(), '{"enabled":true}')
            for name in ("ncd-ob11-stats.cjs", "10001/nodes.json"):
                self.assertEqual((root / name).stat().st_mtime_ns, before[name])

    def test_scope_and_digests_are_validated_before_touching_disk(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            for mutation in ("../business.json", "snowluma-shared/20002.json", "10001/../nodes.json"):
                value = request(home)
                value["assets"][0]["path"] = mutation
                with self.assertRaises(ValueError):
                    BATCH.synchronize(value)
            value = request(home)
            value["assets"][0]["sha256"] = "0" * 64
            with self.assertRaises(ValueError):
                BATCH.synchronize(value)
            self.assertFalse((home / "ncd-watch").exists())

    def test_atomic_replace_failure_keeps_original_and_cleans_temporary_file(self):
        with tempfile.TemporaryDirectory() as directory:
            value = request(directory)
            BATCH.synchronize(value)
            root = Path(directory) / "ncd-watch" / "metrics"
            value["assets"][0] = asset("ncd-ob11-stats.cjs", "new-probe")
            with patch.object(BATCH.os, "replace", side_effect=PermissionError()):
                with self.assertRaises(PermissionError):
                    BATCH.synchronize(value)
            self.assertEqual((root / "ncd-ob11-stats.cjs").read_text(), "probe-fixture")
            self.assertEqual(list(root.glob(".ncd-assets-*")), [])

    def test_changed_asset_preserves_its_permissions(self):
        with tempfile.TemporaryDirectory() as directory:
            value = request(directory)
            BATCH.synchronize(value)
            path = Path(directory) / "ncd-watch/metrics/ncd-ob11-stats.cjs"
            path.chmod(0o640)
            mode = stat.S_IMODE(path.stat().st_mode)
            value["assets"][0] = asset("ncd-ob11-stats.cjs", "new-probe")
            BATCH.synchronize(value)
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), mode)

    def test_symlink_cannot_redirect_an_asset_into_business_files(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            value = request(home)
            BATCH.synchronize(value)
            business = home / "business.json"
            business.write_text("keep")
            target = home / "ncd-watch/metrics/ncd-ob11-stats.cjs"
            target.unlink()
            try:
                target.symlink_to(business)
            except OSError:
                self.skipTest("symlink creation is unavailable on this test host")
            with self.assertRaises(ValueError):
                BATCH.synchronize(value)
            self.assertEqual(business.read_text(), "keep")


if __name__ == "__main__":
    unittest.main()
