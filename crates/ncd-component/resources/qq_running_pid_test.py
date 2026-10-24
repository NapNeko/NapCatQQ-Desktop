"""QQ process ownership and warm-refresh probe regressions."""

import contextlib
import importlib.util
import io
from pathlib import Path
import sys
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "qq_running_pid", Path(__file__).with_name("qq-running-pid.py")
)
PROBE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = PROBE
SPEC.loader.exec_module(PROBE)


def qq(pid=42, args=(), birth="12345", exe="/opt/QQ/qq"):
    return PROBE.Process(pid, birth, ("/opt/QQ/qq", *args), exe)


class QqRunningPidTests(unittest.TestCase):
    def test_pending_single_main_skips_all_ptlogin_requests(self):
        process = qq()
        with patch.object(PROBE, "owned_ptlogin_ports", side_effect=AssertionError("not needed")):
            self.assertEqual(PROBE.select("10001", [process], True, "/opt/QQ/qq"), process)

    def test_multiple_pending_mains_are_not_assigned_to_one_bot(self):
        with patch.object(PROBE, "owned_ptlogin_ports", return_value=set()):
            self.assertIsNone(PROBE.select("10001", [qq(), qq(43)], True, "/opt/QQ/qq"))

    def test_single_main_fallback_is_disabled_for_multiple_configured_bots(self):
        with patch.object(PROBE, "owned_ptlogin_ports", return_value=set()):
            self.assertIsNone(PROBE.select("10001", [qq()], False, "/opt/QQ/qq"))

    def test_explicit_other_account_cannot_use_single_main_fallback(self):
        self.assertIsNone(PROBE.select("10001", [qq(args=("-q", "20002"))], True, "/opt/QQ/qq"))

    def test_exact_account_options_do_not_accept_substrings(self):
        for args in (("-q10001",), ("--qq=10001",), ("--qq-id", "10001")):
            self.assertEqual(PROBE.accounts(qq(args=args)), {"10001"})
        self.assertEqual(PROBE.accounts(qq(args=("--qq=100010",))), {"100010"})

    def test_duplicate_account_processes_are_ambiguous(self):
        with self.assertRaises(ValueError):
            PROBE.select("10001", [qq(args=("-q10001",)), qq(43, ("-q10001",))], True, "")

    def test_stale_pidfile_never_establishes_account_ownership(self):
        correct = qq(43, ("-q10001",))
        with (
            patch.object(PROBE, "snapshot", return_value=([qq(42, ("-q20002",)), correct], False)),
            patch.object(PROBE, "still_current", return_value=True),
            patch.object(PROBE, "read_limited", side_effect=AssertionError("PID file not trusted")),
            contextlib.redirect_stdout(io.StringIO()) as output,
        ):
            PROBE.main(["10001", "/tmp/stale-pid-file", "/opt/QQ/qq", "0"])
        self.assertEqual(output.getvalue(), "43\n")

    def test_birth_change_before_return_is_an_error(self):
        with (
            patch.object(PROBE, "snapshot", return_value=([qq()], False)),
            patch.object(PROBE, "still_current", return_value=False),
            self.assertRaises(ValueError),
        ):
            PROBE.main(["10001", "/tmp/pid", "/opt/QQ/qq", "1"])

    def test_unknown_snapshot_does_not_report_stopped(self):
        with patch.object(PROBE, "snapshot", return_value=([], True)), self.assertRaises(ValueError):
            PROBE.main(["10001", "", "/opt/QQ/qq", "1"])

    def test_unknown_other_main_prevents_unique_process_fallback(self):
        with (
            patch.object(PROBE, "snapshot", return_value=([qq()], True)),
            patch.object(PROBE, "owned_ptlogin_ports", return_value=set()),
            self.assertRaises(ValueError),
        ):
            PROBE.main(["10001", "", "/opt/QQ/qq", "1"])

    def inspect_fixture(self, args=None, exe_error=None, exe="/opt/QQ/qq", births=None):
        argv = args or b"/opt/QQ/qq\0"

        def read(path, _limit):
            return b"qq\n" if path.endswith("/comm") else argv

        with (
            patch.object(PROBE, "read_limited", side_effect=read),
            patch.object(PROBE, "birth", side_effect=births or ["12345", "12345"]),
            patch.object(PROBE.os, "readlink", side_effect=exe_error, return_value=exe),
            patch.object(PROBE.os.path, "realpath", side_effect=lambda value: value),
            patch.object(PROBE.os.path, "isabs", side_effect=lambda value: value.startswith("/")),
        ):
            return PROBE.inspect(42, "/opt/QQ/qq")

    def test_foreign_uid_exe_permission_falls_back_to_exact_absolute_argv(self):
        process = self.inspect_fixture(exe_error=PermissionError())
        self.assertEqual(process, qq(exe=None))

    def test_exe_permission_does_not_accept_another_installation(self):
        self.assertIsNone(self.inspect_fixture(args=b"/other/qq\0", exe_error=PermissionError()))

    def test_known_executable_mismatch_cannot_use_matching_argv(self):
        self.assertIsNone(self.inspect_fixture(exe="/other/qq"))

    def test_helpers_are_excluded_before_single_main_fallback(self):
        self.assertIsNone(self.inspect_fixture(args=b"/opt/QQ/qq\0--type=renderer\0"))

    def test_pid_reuse_during_snapshot_is_rejected(self):
        self.assertIsNone(self.inspect_fixture(births=["12345", "12346"]))

    def test_revalidation_checks_birth_and_account_argv(self):
        for replacement in (qq(birth="12346"), qq(args=("-q20002",))):
            with patch.object(PROBE, "inspect", return_value=replacement):
                self.assertFalse(PROBE.still_current(qq(), "/opt/QQ/qq"))

    def test_ptlogin_result_is_bound_to_owned_listener(self):
        first, second = qq(), qq(43)
        with (
            patch.object(PROBE, "owned_ptlogin_ports", side_effect=lambda process: {4301} if process.pid == 42 else {4303}),
            patch.object(PROBE, "ptlogin_accounts", side_effect=lambda port: {"10001"} if port == 4303 else {"20002"}) as request,
            patch.object(PROBE, "still_current", return_value=True),
        ):
            self.assertEqual(PROBE.select("10001", [first, second], False, ""), second)
        self.assertEqual({call.args[0] for call in request.call_args_list}, {4301, 4303})

    def test_shared_listener_with_two_owners_is_ambiguous(self):
        with (
            patch.object(PROBE, "owned_ptlogin_ports", return_value={4301}),
            patch.object(PROBE, "ptlogin_accounts", return_value={"10001"}),
            patch.object(PROBE, "still_current", return_value=True),
            self.assertRaises(ValueError),
        ):
            PROBE.select("10001", [qq(), qq(43)], False, "")

    def test_ptlogin_account_json_can_wrap_its_list(self):
        self.assertEqual(PROBE.response_accounts({"u": [{"uin": "10001"}, {"uin": 20002}]}), {"10001", "20002"})


if __name__ == "__main__":
    unittest.main()
