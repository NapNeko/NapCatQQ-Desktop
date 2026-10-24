"""Exact host PID identity and pidfd-only QQ stopping regressions."""

import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch
from types import SimpleNamespace


SPEC = importlib.util.spec_from_file_location("qq_process", Path(__file__).with_name("qq-process.py"))
PROBE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PROBE)


def request(action="inspect"):
    return {"v": 1, "action": action, "pid": 42, "start_time": "12345", "qq_bin": "/opt/QQ/qq", "qq_id": 10001}


class QqProcessTests(unittest.TestCase):
    def setUp(self):
        uid = patch.object(PROBE.os, "getuid", return_value=1001, create=True)
        uid.start()
        self.addCleanup(uid.stop)

    def identity(self, argv=b"/opt/QQ/qq\0", executable="/opt/QQ/qq", births=None, action="inspect", target_uid=1001, owner=1001):
        with (
            patch.object(PROBE, "process_birth", side_effect=births or ["12345", "12345"]),
            patch.object(PROBE, "read_limited", side_effect=lambda path, _: (
                ("Uid:\t" + "\t".join([str(target_uid)] * 4) + "\n").encode()
                if path.endswith("/status") else argv)),
            patch.object(PROBE.os, "stat", return_value=SimpleNamespace(st_uid=owner)),
            patch.object(PROBE.os.path, "realpath", side_effect=lambda path: path),
            patch.object(PROBE.os.path, "isabs", side_effect=lambda path: path.startswith("/")),
            patch.object(PROBE.os, "readlink", side_effect=executable if isinstance(executable, Exception) else None, return_value=executable),
        ):
            return PROBE.execute(request(action))["state"]

    def test_attributed_pending_qq_does_not_require_q_argument(self):
        self.assertEqual(self.identity(), "verified")

    def test_executable_permission_is_distinct_from_absence(self):
        self.assertEqual(self.identity(executable=PermissionError()), "permission")

    def test_original_user_snapshot_rechecks_birth_and_argv_without_reading_exe(self):
        self.assertEqual(self.identity(executable=AssertionError("must not read exe"), action="snapshot"), "verified")
        self.assertEqual(self.identity(action="snapshot", target_uid=2002, owner=0), "replaced")

    def test_dumpable_proc_root_owner_does_not_override_matching_status_uids(self):
        self.assertEqual(self.identity(owner=0), "verified")

    def test_birth_change_is_not_the_original_process(self):
        self.assertEqual(self.identity(births=["54321"]), "replaced")

    def test_birth_change_during_inspection_is_rejected(self):
        self.assertEqual(self.identity(births=["12345", "54321"]), "replaced")

    def test_other_account_helper_and_executable_are_rejected(self):
        self.assertEqual(self.identity(argv=b"/opt/QQ/qq\0-q20002\0"), "replaced")
        self.assertEqual(self.identity(argv=b"/opt/QQ/qq\0--type=renderer\0"), "replaced")
        self.assertEqual(self.identity(executable="/other/QQ/qq"), "replaced")

    def test_wrong_absolute_argv_cannot_use_a_matching_executable(self):
        self.assertEqual(self.identity(argv=b"/other/qq\0"), "replaced")

    def stop_fixture(self, waits, inspections=None, send_error=None):
        with (
            patch.object(PROBE, "inspect", side_effect=inspections),
            patch.object(PROBE, "managed_service", return_value=None),
            patch.object(PROBE, "process_birth", side_effect=ProcessLookupError()),
            patch.object(PROBE.os, "pidfd_open", return_value=99, create=True) as opened,
            patch.object(PROBE.signal, "pidfd_send_signal", side_effect=send_error, create=True) as sent,
            patch.object(PROBE.signal, "SIGKILL", 9, create=True),
            patch.object(PROBE.select, "select", side_effect=waits),
            patch.object(PROBE.os, "close") as closed,
        ):
            result = PROBE.execute(request("stop"))
        return result, opened, sent, closed

    def test_term_targets_only_the_pinned_process_descriptor(self):
        result, opened, sent, closed = self.stop_fixture([([99], [], [])])
        self.assertEqual(result["state"], "stopped")
        opened.assert_called_once_with(42, 0)
        sent.assert_called_once_with(99, PROBE.signal.SIGTERM, None, 0)
        closed.assert_called_once_with(99)

    def test_kill_is_used_only_after_term_grace_and_revalidation(self):
        result, _, sent, _ = self.stop_fixture([([], [], []), ([99], [], [])])
        self.assertEqual(result["state"], "stopped")
        self.assertEqual([call.args[1] for call in sent.call_args_list], [PROBE.signal.SIGTERM, 9])
        self.assertTrue(all(call.args[0] == 99 for call in sent.call_args_list))

    def test_replacement_before_force_kill_never_receives_kill(self):
        result, _, sent, _ = self.stop_fixture([([], [], [])], [None, None, PROBE.Replaced()])
        self.assertEqual(result["state"], "replaced")
        self.assertEqual(len(sent.call_args_list), 1)

    def test_initial_wrong_identity_never_opens_a_pidfd(self):
        result, opened, sent, _ = self.stop_fixture([], [PROBE.Replaced()])
        self.assertEqual(result["state"], "replaced")
        opened.assert_not_called()
        sent.assert_not_called()

    def test_permission_and_timeout_are_not_reported_as_stopped(self):
        result, _, _, _ = self.stop_fixture([], send_error=PermissionError())
        self.assertEqual(result["state"], "permission")
        result, _, _, _ = self.stop_fixture([([], [], []), ([], [], [])])
        self.assertEqual(result["state"], "error")

    def test_already_absent_target_is_idempotent_without_signalling(self):
        result, _, sent, _ = self.stop_fixture([], [ProcessLookupError()])
        self.assertEqual(result["state"], "absent")
        sent.assert_not_called()

    def test_request_rejects_unbound_or_dynamic_targets(self):
        for field, value in (("pid", 0), ("start_time", ""), ("start_time", "1;kill"), ("qq_bin", "qq"), ("action", "kill_all")):
            candidate = request()
            candidate[field] = value
            self.assertFalse(PROBE.validate(candidate))

    def test_service_owned_qq_is_rejected_before_any_signal(self):
        with (
            patch.object(PROBE, "inspect"),
            patch.object(PROBE, "managed_service", return_value="original-stack.service"),
            patch.object(PROBE.os, "pidfd_open", create=True) as opened,
        ):
            result = PROBE.execute(request("stop"))
        self.assertEqual(result["state"], "managed_service")
        self.assertEqual(result["unit"], "original-stack.service")
        opened.assert_not_called()

    def test_session_scope_does_not_claim_its_user_manager_ancestor(self):
        value = b"0::/user.slice/user-1001.slice/user@1001.service/app.slice/qq.scope\n"
        with patch.object(PROBE, "read_limited", return_value=value):
            self.assertIsNone(PROBE.managed_service(42))

    def test_ssh_service_ancestor_does_not_block_a_desktop_started_qq(self):
        result = SimpleNamespace(returncode=0, stdout="MainPID=10\nRestart=on-failure\n")
        with (
            patch.object(PROBE, "read_limited", return_value=b"0::/system.slice/ssh.service\n"),
            patch.object(PROBE, "systemctl_path", return_value="/usr/bin/systemctl"),
            patch.object(PROBE.subprocess, "run", return_value=result),
            patch.object(PROBE, "process_metadata", return_value=("100", 1, "sshd")),
        ):
            self.assertIsNone(PROBE.managed_service(42))

    def test_restartable_wrapper_must_be_an_actual_qq_ancestor(self):
        result = SimpleNamespace(returncode=0, stdout="MainPID=10\nRestart=on-failure\n")
        metadata = {10: ("100", 1, "dbus-run-sessio"), 42: ("12345", 11, "qq"), 11: ("101", 10, "bash")}
        with (
            patch.object(PROBE, "read_limited", return_value=b"0::/system.slice/original-stack.service\n"),
            patch.object(PROBE, "systemctl_path", return_value="/usr/bin/systemctl"),
            patch.object(PROBE.subprocess, "run", return_value=result),
            patch.object(PROBE, "process_metadata", side_effect=lambda pid: metadata[pid]),
        ):
            self.assertEqual(PROBE.managed_service(42), "original-stack.service")
            metadata[42] = ("12345", 1, "qq")
            self.assertIsNone(PROBE.managed_service(42))

    def test_missing_supervisor_metadata_cannot_claim_a_live_target_is_absent(self):
        for error in (FileNotFoundError(), ProcessLookupError()):
            with (
                patch.object(PROBE, "inspect"),
                patch.object(PROBE, "managed_service", side_effect=error),
                patch.object(PROBE, "process_birth", return_value="12345") as checked,
                patch.object(PROBE.os, "pidfd_open", create=True) as opened,
            ):
                result = PROBE.execute(request("stop"))
            self.assertEqual(result["state"], "error")
            checked.assert_called_once_with(42)
            opened.assert_not_called()

    def test_missing_systemctl_with_live_target_is_an_error_without_signals(self):
        with (
            patch.object(PROBE, "inspect"),
            patch.object(PROBE, "read_limited", return_value=b"0::/system.slice/original.service\n"),
            patch.object(PROBE, "systemctl_path", return_value="/usr/bin/systemctl"),
            patch.object(PROBE.subprocess, "run", side_effect=FileNotFoundError()) as invoked,
            patch.object(PROBE, "process_birth", return_value="12345"),
            patch.object(PROBE.os, "pidfd_open", create=True) as opened,
        ):
            result = PROBE.execute(request("stop"))
        self.assertEqual(result["state"], "error")
        self.assertEqual(invoked.call_args.args[0][0], "/usr/bin/systemctl")
        opened.assert_not_called()

    def test_cross_uid_never_opens_pidfd_even_with_sudo_or_root_ssh(self):
        for caller_uid, expected_uid, target_uid in ((1001, None, 2002), (0, 1001, 2002), (0, None, 1001)):
            candidate = request("stop")
            if expected_uid is not None:
                candidate["expected_uid"] = expected_uid
            status = ("Uid:\t" + "\t".join([str(target_uid)] * 4) + "\n").encode()
            with (
                patch.object(PROBE.os, "getuid", return_value=caller_uid),
                patch.object(PROBE.os, "stat", return_value=SimpleNamespace(st_uid=0)),
                patch.object(PROBE, "process_birth", return_value="12345"),
                patch.object(PROBE, "read_limited", return_value=status),
                patch.object(PROBE.os, "pidfd_open", create=True) as opened,
                patch.object(PROBE.signal, "pidfd_send_signal", create=True) as sent,
            ):
                result = PROBE.execute(candidate)
            self.assertEqual(result["state"], "replaced")
            self.assertEqual(result["uid"], caller_uid)
            opened.assert_not_called()
            sent.assert_not_called()

    def test_root_ssh_and_elevated_snapshot_keep_their_original_expected_uid(self):
        with patch.object(PROBE.os, "getuid", return_value=0):
            self.assertEqual(self.identity(target_uid=0, owner=0), "verified")
        candidate = request("snapshot")
        candidate["expected_uid"] = 1001
        with (
            patch.object(PROBE.os, "getuid", return_value=0),
            patch.object(PROBE, "process_birth", return_value="12345"),
            patch.object(PROBE.os, "stat", return_value=SimpleNamespace(st_uid=0)),
            patch.object(PROBE, "read_limited", side_effect=lambda path, _: (
                b"Uid:\t1001\t1001\t1001\t1001\n" if path.endswith("/status") else b"/opt/QQ/qq\0")),
            patch.object(PROBE.os.path, "realpath", side_effect=lambda path: path),
        ):
            result = PROBE.execute(candidate)
        self.assertEqual(result["state"], "verified")
        self.assertEqual(result["uid"], 0)


if __name__ == "__main__":
    unittest.main()
