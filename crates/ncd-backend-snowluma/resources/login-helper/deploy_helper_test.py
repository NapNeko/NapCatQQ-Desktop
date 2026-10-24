import base64
import importlib.util
import os
import pathlib
import stat
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("deploy_helper", pathlib.Path(__file__).with_name("deploy-helper.py"))
DEPLOY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DEPLOY)


class DeploymentTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="ncd-login-deploy-fixture-")
        self.addCleanup(self.temporary.cleanup)
        self.runtime = pathlib.Path(self.temporary.name) / "runtime"
        self.runtime.mkdir()
        self.directory = self.runtime / "login-assist" / "123456"
        self.target = self.directory / "atspi-login.py"
        if not hasattr(os, "getuid"):
            patch = mock.patch.object(DEPLOY.os, "getuid", return_value=self.runtime.stat().st_uid, create=True)
            patch.start()
            self.addCleanup(patch.stop)
        if not hasattr(os, "fchmod"):
            patch = mock.patch.object(DEPLOY.os, "fchmod", create=True)
            patch.start()
            self.addCleanup(patch.stop)

    def request(self, body=b"print('fixture')\n"):
        return {"v": 1, "directory": str(self.directory), "body": base64.b64encode(body).decode()}

    def test_unchanged_helper_keeps_inode_and_only_changed_content_is_replaced(self):
        self.assertTrue(DEPLOY.deploy(self.request())["changed"])
        before = self.target.stat()
        self.assertFalse(DEPLOY.deploy(self.request())["changed"])
        after = self.target.stat()
        self.assertEqual((before.st_ino, before.st_mtime_ns), (after.st_ino, after.st_mtime_ns))
        self.assertTrue(DEPLOY.deploy(self.request(b"updated-fixture"))["changed"])
        self.assertEqual(self.target.read_bytes(), b"updated-fixture")
        self.assertEqual(sorted(path.name for path in self.directory.iterdir()), ["atspi-login.py"])
        if os.name == "posix":
            self.assertEqual(stat.S_IMODE(self.directory.stat().st_mode), 0o700)
            self.assertEqual(stat.S_IMODE(self.target.stat().st_mode), 0o600)
            self.assertEqual(self.target.stat().st_uid, os.getuid())

    def test_atomic_replace_failure_preserves_existing_helper_and_cleans_temp(self):
        DEPLOY.deploy(self.request())
        original = self.target.read_bytes()
        with mock.patch.object(DEPLOY.os, "replace", side_effect=PermissionError("fixture")):
            with self.assertRaises(PermissionError):
                DEPLOY.deploy(self.request(b"replacement-fixture"))
        self.assertEqual(self.target.read_bytes(), original)
        self.assertEqual(sorted(path.name for path in self.directory.iterdir()), ["atspi-login.py"])

    def test_invalid_scope_and_payload_never_create_helper(self):
        for directory in [self.runtime / "other" / "123456", self.directory / ".." / "123456", self.runtime / "login-assist" / "not-a-bot"]:
            request = self.request()
            request["directory"] = str(directory)
            with self.assertRaises(ValueError):
                DEPLOY.deploy(request)
        request = self.request()
        request["body"] = "not valid base64"
        with self.assertRaises(ValueError):
            DEPLOY.deploy(request)
        self.assertEqual(list(self.runtime.iterdir()), [])

    def test_read_permission_error_cannot_replace_helper(self):
        DEPLOY.deploy(self.request())
        original = self.target.read_bytes()
        with mock.patch.object(DEPLOY.os, "open", side_effect=PermissionError("fixture")):
            with self.assertRaises(PermissionError):
                DEPLOY.deploy(self.request(b"replacement-fixture"))
        self.assertEqual(self.target.read_bytes(), original)

    def test_symlink_helper_cannot_overwrite_another_file(self):
        DEPLOY.deploy(self.request())
        self.target.unlink()
        unrelated = self.runtime / "unrelated-fixture"
        unrelated.write_bytes(b"keep-fixture")
        try:
            self.target.symlink_to(unrelated)
        except OSError:
            self.skipTest("symlink creation unavailable")
        with self.assertRaises((OSError, ValueError)):
            DEPLOY.deploy(self.request(b"replacement-fixture"))
        self.assertEqual(unrelated.read_bytes(), b"keep-fixture")


if __name__ == "__main__":
    unittest.main()
