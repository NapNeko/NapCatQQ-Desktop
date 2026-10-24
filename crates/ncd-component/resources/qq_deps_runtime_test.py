"""Execute the readonly runtime package probe with isolated package-query fixtures."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import unittest


SCRIPT = Path(__file__).with_name("qq-deps-runtime.sh").read_text(encoding="utf-8")
BASH = shutil.which("bash")
if os.name == "nt":
    git_bash = Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "Git/bin/bash.exe"
    if git_bash.is_file():
        BASH = str(git_bash)


@unittest.skipUnless(BASH, "bash is required to execute the shell fixture")
class RuntimeBatchTests(unittest.TestCase):
    def run_probe(self, family, packages, commands):
        args = [value for name, alias in packages for value in (name, str(int(alias)))]
        return subprocess.run(
            [BASH, "-c", commands + "\n" + SCRIPT, "qq-runtime-fixture", family, *args],
            capture_output=True, text=True, timeout=5, check=False,
        )

    def packages(self, result):
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["v"], 1)
        return payload["packages"]

    def test_available_t64_alias_and_missing_package(self):
        result = self.run_probe("debian", [("libglib2.0-0", True), ("curl", False)], r'''
apt-cache() { [ "$2" = libglib2.0-0t64 ]; }
dpkg-query() {
    case "$3" in
        libglib2.0-0t64) printf 'install ok installed' ;;
        curl) return 1 ;;
        *) return 2 ;;
    esac
}
''')
        self.assertEqual(self.packages(result), [
            {"base": "libglib2.0-0", "name": "libglib2.0-0t64", "installed": True},
            {"base": "curl", "name": "curl", "installed": False},
        ])

    def test_unavailable_alias_keeps_installed_base_package(self):
        result = self.run_probe("debian", [("libglib2.0-0", True)], r'''
apt-cache() { return 100; }
dpkg-query() { [ "$3" = libglib2.0-0 ] || return 2; printf 'install ok installed'; }
''')
        self.assertEqual(self.packages(result)[0], {
            "base": "libglib2.0-0", "name": "libglib2.0-0", "installed": True,
        })

    def test_config_files_only_does_not_count_as_installed(self):
        result = self.run_probe("debian", [("curl", False)], r'''
dpkg-query() { printf 'deinstall ok config-files'; }
''')
        self.assertFalse(self.packages(result)[0]["installed"])

    def test_package_query_failure_cannot_return_a_complete_report(self):
        result = self.run_probe("debian", [("curl", False)], r'''
dpkg-query() { return 2; }
''')
        self.assertNotEqual(result.returncode, 0)

    def test_alias_query_failure_is_not_missing(self):
        result = self.run_probe("debian", [("libglib2.0-0", True)], r'''
apt-cache() { return 127; }
dpkg-query() { printf 'install ok installed'; }
''')
        self.assertNotEqual(result.returncode, 0)

    def test_rpm_installed_and_missing_remain_distinct(self):
        result = self.run_probe("rhel", [("nss", False), ("mesa-libgbm", False)], r'''
rpm() {
    case "$2" in
        nss) printf 'nss-1.2.3.x86_64' ;;
        mesa-libgbm) printf 'package mesa-libgbm is not installed'; return 1 ;;
        *) return 2 ;;
    esac
}
''')
        self.assertEqual([item["installed"] for item in self.packages(result)], [True, False])

    def test_rpm_database_error_is_not_a_missing_package(self):
        result = self.run_probe("rhel", [("nss", False)], r'''
rpm() { printf 'rpmdb is unavailable' >&2; return 1; }
''')
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
