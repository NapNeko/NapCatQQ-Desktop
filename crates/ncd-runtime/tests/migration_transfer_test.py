"""Exercise migration archives without SSH or changes to real Bot data."""

from contextlib import closing, redirect_stdout
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sqlite3
import stat
import subprocess
import sys
import tarfile
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock


SCRIPT = (
    Path(__file__).resolve().parents[1]
    / "src/app_framework/manager/migration_transfer.py"
)
SPEC = importlib.util.spec_from_file_location("ncd_migration_transfer_under_test", SCRIPT)
TRANSFER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TRANSFER)


class MigrationTransferTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="ncd-migration-test-")
        self.addCleanup(temporary.cleanup)
        self.workspace = Path(temporary.name).resolve()
        self.source = self.workspace / "source"
        self.source.mkdir()
        self.managed = self.workspace / "managed"
        self.managed.mkdir()
        self.target = self.managed / "migrated-bot"
        self.archive = self.workspace / "snapshot.tar.gz"
        no_writers = mock.patch.object(TRANSFER, "no_writers")
        no_writers.start()
        self.addCleanup(no_writers.stop)
        free_space = mock.patch.object(
            TRANSFER.shutil,
            "disk_usage",
            return_value=SimpleNamespace(free=10 * 1024**3),
        )
        free_space.start()
        self.addCleanup(free_space.stop)

    def write_source(self, relative, contents):
        path = self.source / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(contents, str):
            contents = contents.encode("utf-8")
        path.write_bytes(contents)
        return path

    def pack_source(self):
        return TRANSFER.pack(self.source, self.archive)

    def restore(self, expected=None, old_root=None, target=None):
        return TRANSFER.restore(
            target or self.target,
            self.archive,
            expected or TRANSFER.digest(self.archive),
            old_root or self.source.as_posix(),
            str(self.managed),
        )

    def stage(self, directory):
        arguments = [str(SCRIPT), "stage", str(directory), str(self.managed)]
        with mock.patch.object(TRANSFER.sys, "argv", arguments), redirect_stdout(io.StringIO()) as output:
            TRANSFER.main()
        return json.loads(output.getvalue())

    def write_archive(self, entries, rows=None):
        if rows is None:
            rows = {}
            for member, contents in entries:
                if member.isfile():
                    rows[member.name] = {
                        "bytes": len(contents),
                        "sha256": hashlib.sha256(contents).hexdigest(),
                    }
        with tarfile.open(self.archive, "w:gz") as bundle:
            for member, contents in entries:
                if contents is not None:
                    member.size = len(contents)
                bundle.addfile(member, io.BytesIO(contents) if contents is not None else None)
            payload = json.dumps(rows).encode("utf-8")
            manifest = tarfile.TarInfo(TRANSFER.MANIFEST)
            manifest.size = len(payload)
            bundle.addfile(manifest, io.BytesIO(payload))

    def test_business_data_round_trip_with_valid_sqlite_and_hashes(self):
        self.write_source("bot.py", "print('migration fixture')\n")
        self.write_source("plugins/custom_plugin/__init__.py", "enabled = True\n")
        self.write_source("data/player.json", '{"level": 42}')
        self.write_source(".env", "PORT=8080\nTOKEN=fixture-only\n")
        database_path = self.source / "data" / "players.sqlite"
        with closing(sqlite3.connect(database_path)) as database:
            database.execute("CREATE TABLE players (name TEXT, level INTEGER)")
            database.execute("INSERT INTO players VALUES (?, ?)", ("测试玩家", 42))
            database.commit()

        packed = self.pack_source()
        restored = self.restore(packed["sha256"])

        self.assertEqual(packed["sha256"], TRANSFER.digest(self.archive))
        self.assertEqual(restored["sha256"], packed["sha256"])
        self.assertEqual(restored["files"], packed["files"])
        for relative in (
            "bot.py",
            "plugins/custom_plugin/__init__.py",
            "data/player.json",
            ".env",
            "data/players.sqlite",
        ):
            with self.subTest(file=relative):
                self.assertEqual(
                    TRANSFER.digest(self.target / relative),
                    TRANSFER.digest(self.source / relative),
                )
        with closing(sqlite3.connect(self.target / "data" / "players.sqlite")) as database:
            self.assertEqual(database.execute("SELECT * FROM players").fetchall(), [("测试玩家", 42)])
            self.assertEqual(database.execute("PRAGMA quick_check").fetchall(), [("ok",)])

    def test_committed_sqlite_wal_is_preserved(self):
        database_path = self.source / "business.sqlite"
        with closing(sqlite3.connect(database_path)) as database:
            self.assertEqual(database.execute("PRAGMA journal_mode=WAL").fetchone(), ("wal",))
            database.execute("PRAGMA wal_autocheckpoint=0")
            database.execute("CREATE TABLE messages (body TEXT)")
            database.execute("INSERT INTO messages VALUES ('committed in WAL')")
            database.commit()
            self.assertGreater(Path(str(database_path) + "-wal").stat().st_size, 0)
            self.pack_source()
            self.restore()
            with closing(sqlite3.connect(self.target / "business.sqlite")) as restored:
                self.assertEqual(
                    restored.execute("SELECT body FROM messages").fetchall(),
                    [("committed in WAL",)],
                )
                self.assertEqual(restored.execute("PRAGMA quick_check").fetchall(), [("ok",)])

    def test_rebuildable_dependencies_are_excluded_at_every_depth(self):
        self.write_source("bot.py", "pass\n")
        for relative in (
            "node_modules/pkg/index.js",
            ".venv/lib/package.py",
            "plugins/local/node_modules/pkg/index.js",
            "plugins/local/.venv/lib/package.py",
            "plugins/local/__pycache__/cached.pyc",
        ):
            self.write_source(relative, b"rebuildable")
        self.pack_source()
        with tarfile.open(self.archive, "r:gz") as bundle:
            files = {member.name for member in bundle.getmembers() if member.isfile()}
            self.assertEqual(files, {"bot.py", TRANSFER.MANIFEST})
        self.restore()
        self.assertEqual((self.target / "bot.py").read_text(encoding="utf-8"), "pass\n")
        self.assertFalse((self.target / "node_modules").exists())
        self.assertFalse((self.target / ".venv").exists())

    def test_empty_business_directories_are_preserved(self):
        (self.source / "data" / "empty" / "nested").mkdir(parents=True)
        (self.source / "plugins" / "empty_plugin").mkdir(parents=True)
        self.write_source("bot.py", "pass\n")

        self.pack_source()
        self.restore()

        for relative in ("data/empty/nested", "plugins/empty_plugin"):
            with self.subTest(directory=relative):
                restored = self.target / relative
                self.assertTrue(restored.is_dir())
                self.assertEqual(list(restored.iterdir()), [])

    def test_staging_directory_is_reusable_for_multiple_app_archives(self):
        staging = self.managed / "migrations" / "one-job"
        self.assertEqual(self.stage(staging)["files"], 0)
        self.write_source("bot.py", "first app\n")
        first_archive = staging / "app-a.tar.gz"
        first = TRANSFER.pack(self.source, first_archive)

        self.stage(staging)
        second_source = self.workspace / "second-app"
        second_source.mkdir()
        (second_source / "bot.py").write_text("second app\n", encoding="utf-8")
        second_archive = staging / "app-b.tar.gz"
        second = TRANSFER.pack(second_source, second_archive)
        self.stage(staging)

        self.assertEqual(TRANSFER.digest(first_archive), first["sha256"])
        self.assertEqual(TRANSFER.digest(second_archive), second["sha256"])
        self.assertNotEqual(first["sha256"], second["sha256"])
        self.assertEqual({path.name for path in staging.iterdir()}, {"app-a.tar.gz", "app-b.tar.gz"})

    @unittest.skipUnless(os.name == "posix", "POSIX permission bits do not apply on Windows")
    def test_staging_and_archives_have_private_posix_permissions(self):
        staging = self.managed / "migrations" / "one-job"
        staging.mkdir(parents=True)
        staging.chmod(0o777)
        self.stage(staging)
        self.assertEqual(stat.S_IMODE(staging.stat().st_mode), 0o700)

        secret = self.write_source(".env", "TOKEN=fixture-only\n")
        secret.chmod(0o600)
        packed = TRANSFER.pack(self.source, self.archive)
        self.assertEqual(stat.S_IMODE(self.archive.stat().st_mode), 0o600)
        self.restore(expected=packed["sha256"])
        self.assertEqual(stat.S_IMODE(self.target.stat().st_mode), 0o700)
        self.assertEqual(stat.S_IMODE((self.target / ".env").stat().st_mode), 0o600)

    def test_staging_outside_managed_root_is_rejected(self):
        outside = self.workspace / "outside-staging"
        with self.assertRaises(ValueError):
            self.stage(outside)
        self.assertFalse(outside.exists())

    def test_dotenv_paths_below_old_root_are_rewritten(self):
        old_root = "/srv/old-bot"
        self.write_source(".env", f"DATABASE={old_root}/data/main.sqlite\nTOKEN=fixture\n")
        self.pack_source()
        self.restore(old_root=old_root)
        self.assertEqual(
            (self.target / ".env").read_text(encoding="utf-8"),
            f"DATABASE={self.target}/data/main.sqlite\nTOKEN=fixture\n",
        )

    def test_dotenv_exact_root_value_is_rewritten(self):
        old_root = "/srv/old-bot"
        self.write_source(".env", f"BOT_ROOT={old_root}\n")
        self.pack_source()
        self.restore(old_root=old_root)
        self.assertEqual(
            (self.target / ".env").read_text(encoding="utf-8"),
            f"BOT_ROOT={self.target}\n",
        )

    def test_path_rewrite_preserves_other_roots_and_uri_prefixes(self):
        old_root = "/srv/old-bot"
        content = (
            f"ROOT={old_root}\n"
            f"DATABASE=sqlite:///{old_root}/data/main.sqlite\n"
            f"FILE=file://{old_root}/data/config.json\n"
            f"BACKUP=/backup{old_root}/data/main.sqlite\n"
            f"SIBLING={old_root}-backup/data/main.sqlite\n"
            f"REMOTE=https://example.invalid{old_root}/data/main.sqlite\n"
        )
        self.write_source(".env", content)
        self.pack_source()
        self.restore(old_root=old_root)
        self.assertEqual(
            (self.target / ".env").read_text(encoding="utf-8"),
            f"ROOT={self.target}\n"
            f"DATABASE=sqlite:///{self.target}/data/main.sqlite\n"
            f"FILE=file://{self.target}/data/config.json\n"
            f"BACKUP=/backup{old_root}/data/main.sqlite\n"
            f"SIBLING={old_root}-backup/data/main.sqlite\n"
            f"REMOTE=https://example.invalid{old_root}/data/main.sqlite\n",
        )

    def test_pack_rejects_project_symlinks(self):
        outside = self.workspace / "outside.txt"
        outside.write_text("outside project", encoding="utf-8")
        try:
            (self.source / "linked.txt").symlink_to(outside)
        except (OSError, NotImplementedError) as error:
            self.skipTest(f"File symlinks unavailable on this machine: {error}")
        with self.assertRaises(ValueError):
            self.pack_source()
        self.assertFalse(self.archive.exists())

    def test_restore_rejects_tar_path_traversal(self):
        for name in ("../escaped.txt", "/escaped.txt", "nested/../../escaped.txt", "nested\\escaped.txt"):
            with self.subTest(name=name):
                self.write_archive([(tarfile.TarInfo(name), b"must not escape")])
                with self.assertRaises(ValueError):
                    self.restore()
                self.assertFalse(self.target.exists())
                self.assertFalse((self.workspace / "escaped.txt").exists())

    def test_restore_rejects_tar_symlinks_and_hardlinks(self):
        for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE):
            with self.subTest(type=kind):
                member = tarfile.TarInfo("linked.txt")
                member.type = kind
                member.linkname = "../outside.txt"
                self.write_archive([(member, None)])
                with self.assertRaises(ValueError):
                    self.restore()
                self.assertFalse(self.target.exists())

    def test_restore_rejects_duplicate_tar_members(self):
        self.write_archive(
            [(tarfile.TarInfo("bot.py"), b"first"), (tarfile.TarInfo("bot.py"), b"second")]
        )
        with self.assertRaises(ValueError):
            self.restore()
        self.assertFalse(self.target.exists())

    def test_existing_target_is_never_overwritten(self):
        self.write_source("bot.py", "new data")
        self.pack_source()
        self.target.mkdir()
        marker = self.target / "bot.py"
        marker.write_text("existing data", encoding="utf-8")
        with self.assertRaises(ValueError):
            self.restore()
        self.assertEqual(marker.read_text(encoding="utf-8"), "existing data")

    def test_bad_archive_hash_is_rejected_before_target_creation(self):
        self.write_source("bot.py", "pass")
        self.pack_source()
        with self.assertRaises(ValueError):
            self.restore(expected="0" * 64)
        self.assertFalse(self.target.exists())

    def test_bad_manifest_file_hash_is_rejected(self):
        self.write_archive(
            [(tarfile.TarInfo("bot.py"), b"pass")],
            rows={"bot.py": {"bytes": 4, "sha256": "0" * 64}},
        )
        with self.assertRaises(ValueError):
            self.restore()

    def test_corrupt_sqlite_is_rejected(self):
        self.write_source("business.sqlite", b"SQLite format 3\x00" + b"\x00" * 4096)
        self.pack_source()
        with self.assertRaises((ValueError, sqlite3.DatabaseError)):
            self.restore()

    def test_target_outside_managed_root_is_rejected(self):
        self.write_source("bot.py", "pass")
        self.pack_source()
        outside_target = self.workspace / "outside-managed-root"
        with self.assertRaises(ValueError):
            self.restore(target=outside_target)
        self.assertFalse(outside_target.exists())


@unittest.skipUnless(sys.platform.startswith("linux"), "Writer detection uses Linux /proc")
class ProjectWriterTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="ncd-migration-writer-test-")
        self.addCleanup(temporary.cleanup)
        self.workspace = Path(temporary.name).resolve()
        self.source = self.workspace / "source"
        self.source.mkdir()
        self.data = self.source / "business.sqlite"
        self.data.write_bytes(b"fixture")

    def process_with_open_file(self, path, mode, cwd):
        process = subprocess.Popen(
            [sys.executable, "-c",
             "import sys; stream=open(sys.argv[1], sys.argv[2]); print('ready', flush=True); sys.stdin.read(1)",
             str(path), mode],
            cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True,
        )
        def cleanup():
            process.terminate()
            process.communicate(timeout=5)
        self.addCleanup(cleanup)
        self.assertEqual(process.stdout.readline().strip(), "ready")
        return process

    def test_external_working_directory_with_writable_project_file_blocks_snapshot(self):
        for mode in ("a", "r+"):
            with self.subTest(mode=mode):
                process = self.process_with_open_file(self.data, mode, self.workspace)
                with self.assertRaisesRegex(ValueError, str(process.pid)):
                    TRANSFER.no_writers(self.source)
                process.terminate()
                process.communicate(timeout=5)

    def test_readonly_project_file_does_not_block_snapshot(self):
        self.process_with_open_file(self.data, "r", self.workspace)
        TRANSFER.no_writers(self.source)

    def test_writable_file_in_sibling_directory_does_not_block_snapshot(self):
        sibling = self.workspace / "source-backup"
        sibling.mkdir()
        self.process_with_open_file(sibling / "business.sqlite", "a", self.workspace)
        TRANSFER.no_writers(self.source)

    def test_project_working_directory_still_blocks_without_a_writable_file(self):
        process = self.process_with_open_file(self.data, "r", self.source)
        with self.assertRaisesRegex(ValueError, str(process.pid)):
            TRANSFER.no_writers(self.source)


if __name__ == "__main__":
    unittest.main()
