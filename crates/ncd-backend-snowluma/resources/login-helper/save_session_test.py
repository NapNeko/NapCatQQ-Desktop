import base64
import importlib.util
import io
import json
import os
import pathlib
import runpy
import stat
import tempfile
import unittest
from types import SimpleNamespace
from unittest import mock

SPEC = importlib.util.spec_from_file_location("save_session", pathlib.Path(__file__).with_name("save-session.py"))
SAVE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SAVE)


class SessionSaveTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="ncd-login-session-fixture-")
        self.addCleanup(self.temporary.cleanup)
        self.runtime = pathlib.Path(self.temporary.name) / "runtime"
        self.assist = self.runtime / "login-assist"
        self.directory = self.assist / "123456"
        self.directory.mkdir(parents=True)
        self.assist.chmod(0o700)
        self.directory.chmod(0o700)
        self.target = self.directory / "session.json"
        if not hasattr(os, "getuid"):
            patch = mock.patch.object(SAVE.os, "getuid", return_value=self.runtime.stat().st_uid, create=True)
            patch.start(); self.addCleanup(patch.stop)
        if not hasattr(os, "fchmod"):
            patch = mock.patch.object(SAVE.os, "fchmod", create=True)
            patch.start(); self.addCleanup(patch.stop)
        if os.name != "posix":
            patch = mock.patch.object(SAVE.stat, "S_IMODE", return_value=0o700)
            patch.start(); self.addCleanup(patch.stop)

    def session(self):
        return {"directory": str(self.directory), "expected_uin": "123456",
                "session_id": "a" * 32, "qq_pid": 19, "binding": {"pid": 19}}

    def request(self, session=None):
        body = json.dumps(self.session() if session is None else session).encode()
        return {"v": 1, "directory": str(self.directory), "body": base64.b64encode(body).decode()}

    def assert_only_original(self, original):
        self.assertEqual(self.target.read_bytes(), original)
        self.assertEqual(sorted(path.name for path in self.directory.iterdir()), ["session.json"])

    def test_session_is_replaced_atomically_with_private_permissions(self):
        self.assertEqual(SAVE.save(self.request()), {"v": 1, "saved": True})
        previous = self.target.stat().st_ino
        session = self.session(); session["session_id"] = "b" * 32
        replace = SAVE.os.replace
        def inspect_replace(source, target):
            self.assertEqual(json.loads(self.target.read_bytes())["session_id"], "a" * 32)
            self.assertEqual(json.loads(pathlib.Path(source).read_bytes())["session_id"], "b" * 32)
            if os.name == "posix":
                self.assertEqual(stat.S_IMODE(pathlib.Path(source).stat().st_mode), 0o600)
            return replace(source, target)
        with mock.patch.object(SAVE.os, "replace", side_effect=inspect_replace):
            SAVE.save(self.request(session))
        self.assertEqual(json.loads(self.target.read_bytes()), session)
        self.assertNotEqual(self.target.stat().st_ino, previous)
        self.assertEqual(sorted(path.name for path in self.directory.iterdir()), ["session.json"])
        if os.name == "posix":
            self.assertEqual(stat.S_IMODE(self.target.stat().st_mode), 0o600)

    def test_failed_fsync_or_replace_preserves_original_and_removes_temporary(self):
        SAVE.save(self.request())
        original = self.target.read_bytes()
        for operation in ("fsync", "replace"):
            with self.subTest(operation=operation), mock.patch.object(
                    SAVE.os, operation, side_effect=PermissionError("fixture")):
                with self.assertRaises(PermissionError):
                    SAVE.save(self.request())
            self.assert_only_original(original)

    def test_invalid_scope_identity_and_body_never_replace_session(self):
        SAVE.save(self.request())
        original = self.target.read_bytes()
        requests = []
        for directory in [self.runtime / "other" / "123456", self.directory / ".." / "123456",
                          self.assist / "not-a-bot", pathlib.Path("runtime/login-assist/123456")]:
            request = self.request(); request["directory"] = str(directory); requests.append(request)
        for key, value in [("directory", str(self.assist / "987654")), ("expected_uin", "987654"),
                           ("session_id", "g" * 32), ("session_id", "a" * 31),
                           ("qq_pid", 0), ("qq_pid", True), ("binding", {"pid": 20}),
                           ("binding", {"pid": True})]:
            session = self.session(); session[key] = value; requests.append(self.request(session))
        for body in ["not valid base64", base64.b64encode(b"{}" * 5000).decode(),
                     base64.b64encode(b"[]").decode(), base64.b64encode(b"invalid-json").decode()]:
            request = self.request(); request["body"] = body; requests.append(request)
        for request in requests:
            with self.subTest(request=request), self.assertRaises((ValueError, TypeError)):
                SAVE.save(request)
            self.assert_only_original(original)

    def test_missing_directories_are_not_created(self):
        self.directory.rmdir(); self.assist.rmdir()
        with self.assertRaises(FileNotFoundError):
            SAVE.save(self.request())
        self.assertEqual(list(self.runtime.iterdir()), [])
        self.runtime.rmdir()
        with self.assertRaises(FileNotFoundError):
            SAVE.save(self.request())
        self.assertFalse(self.runtime.exists())

    def test_body_limit_applies_to_decoded_bytes(self):
        session = self.session(); session["padding"] = ""
        size = len(json.dumps(session).encode())
        session["padding"] = "x" * (8192 - size)
        self.assertTrue(SAVE.save(self.request(session))["saved"])
        original = self.target.read_bytes()
        session["padding"] += "x"
        with self.assertRaises(ValueError):
            SAVE.save(self.request(session))
        self.assert_only_original(original)

    @unittest.skipUnless(os.name == "posix", "POSIX directory permissions")
    def test_non_private_directories_are_rejected_without_chmod(self):
        for directory in (self.assist, self.directory):
            directory.chmod(0o755)
            with self.assertRaises(ValueError):
                SAVE.save(self.request())
            self.assertEqual(stat.S_IMODE(directory.stat().st_mode), 0o755)
            directory.chmod(0o700)
        self.assertFalse(self.target.exists())

    def test_foreign_directory_or_session_owner_is_rejected(self):
        SAVE.save(self.request())
        original = self.target.read_bytes()
        lstat = pathlib.Path.lstat
        for foreign in (self.assist, self.directory, self.target):
            def metadata(path, *args, **kwargs):
                info = lstat(path, *args, **kwargs)
                if path == foreign:
                    return SimpleNamespace(st_mode=info.st_mode, st_uid=info.st_uid + 1,
                                           st_nlink=info.st_nlink)
                return info
            with self.subTest(path=foreign), mock.patch.object(pathlib.Path, "lstat", metadata):
                with self.assertRaises(ValueError):
                    SAVE.save(self.request())
            self.assert_only_original(original)

    def test_symlink_directory_or_target_cannot_escape_scope(self):
        outside = pathlib.Path(self.temporary.name) / "outside"
        outside.mkdir(); outside.chmod(0o700)
        unrelated = outside / "unrelated"
        unrelated.write_bytes(b"keep-fixture")
        try:
            self.target.symlink_to(unrelated)
        except OSError:
            self.skipTest("symlink creation unavailable")
        with self.assertRaises((OSError, ValueError)):
            SAVE.save(self.request())
        self.assertEqual(unrelated.read_bytes(), b"keep-fixture")
        self.target.unlink(); self.directory.rmdir()
        self.directory.symlink_to(outside, target_is_directory=True)
        with self.assertRaises((OSError, ValueError)):
            SAVE.save(self.request())
        self.assertFalse((outside / "session.json").exists())

    def test_hardlinked_target_cannot_replace_or_modify_another_file(self):
        unrelated = self.runtime / "unrelated"
        unrelated.write_bytes(b"keep-fixture")
        try:
            os.link(unrelated, self.target)
        except OSError:
            self.skipTest("hardlink creation unavailable")
        with self.assertRaises(ValueError):
            SAVE.save(self.request())
        self.assertEqual(self.target.read_bytes(), b"keep-fixture")
        self.assertEqual(unrelated.read_bytes(), b"keep-fixture")

    def test_non_regular_target_is_rejected(self):
        self.target.mkdir()
        with self.assertRaises(ValueError):
            SAVE.save(self.request())
        self.assertTrue(self.target.is_dir())

    def test_symlink_assist_and_runtime_are_rejected(self):
        outside = pathlib.Path(self.temporary.name) / "outside"
        outside.mkdir(); outside.chmod(0o700)
        self.directory.rmdir(); self.assist.rmdir()
        try:
            self.assist.symlink_to(outside, target_is_directory=True)
        except OSError:
            self.skipTest("symlink creation unavailable")
        with self.assertRaises((OSError, ValueError)):
            SAVE.save(self.request())
        self.assist.unlink(); self.runtime.rmdir()
        self.runtime.symlink_to(outside, target_is_directory=True)
        with self.assertRaises((OSError, ValueError)):
            SAVE.save(self.request())
        self.assertEqual(list(outside.iterdir()), [])

    def test_cli_failure_never_prints_session_material(self):
        session = self.session(); session["session_id"] = "PRIVATE-FIXTURE-MATERIAL"
        stdin = SimpleNamespace(buffer=io.BytesIO(json.dumps(self.request(session)).encode()))
        stdout, stderr = io.StringIO(), io.StringIO()
        with mock.patch("sys.stdin", stdin), mock.patch("sys.stdout", stdout), \
                mock.patch("sys.stderr", stderr), self.assertRaises(SystemExit) as stopped:
            runpy.run_path(SPEC.origin, run_name="__main__")
        self.assertEqual(stopped.exception.code, 2)
        self.assertEqual(stdout.getvalue(), "")
        self.assertEqual(stderr.getvalue(), "NCD login session save failed\n")


if __name__ == "__main__":
    unittest.main()
