import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('metadata', Path(__file__).with_name('novnc-metadata.py'))
metadata = importlib.util.module_from_spec(spec)
spec.loader.exec_module(metadata)


def process_stat(birth='12345'):
    fields = [b'S'] * 20
    fields[19] = birth.encode()
    return b'42 (QQ (main)) ' + b' '.join(fields)


def target_process(display=':2'):
    files = {name: mock.Mock() for name in ['stat', 'exe', 'cmdline', 'environ']}
    files['stat'].read_bytes.return_value = process_stat()
    files['exe'].resolve.return_value = Path('/opt/QQ/qq').resolve()
    files['cmdline'].read_bytes.return_value = b'\0'.join([b'qq', b'-q', b'10001']) + b'\0'
    files['environ'].read_bytes.return_value = ('DISPLAY=' + display + '\0').encode()
    proc = mock.Mock()
    proc.stat.return_value = SimpleNamespace(st_uid=1000)
    proc.joinpath.side_effect = files.__getitem__
    root = mock.Mock()
    root.joinpath.return_value = proc
    return root, proc, files


class MetadataTests(unittest.TestCase):
    def inspect(self, root):
        return metadata.inspect_target(Path('/opt/QQ/qq'), 42, '12345', 10001, 1000,
                                       proc_root=root)

    def test_reads_only_attributed_process_and_returns_only_session_metadata(self):
        root, _, files = target_process(':106')
        files['environ'].read_bytes.return_value += b'PRIVATE_TOKEN=never-return-this\0XAUTHORITY=/home/user/.Xauthority\0'
        result = self.inspect(root)
        self.assertEqual(result['display'], ':106')
        self.assertEqual(result['uid'], 1000)
        self.assertEqual(result['xauthority'], '/home/user/.Xauthority')
        self.assertEqual(set(result), {'pid', 'uid', 'start_time', 'exe', 'display', 'xauthority'})
        self.assertNotIn('never-return-this', json.dumps(result))
        root.joinpath.assert_called_once_with('42')

    def test_expected_owner_is_checked_even_when_helper_can_read_other_users(self):
        root, proc, files = target_process()
        proc.stat.return_value.st_uid = 2000
        with self.assertRaisesRegex(metadata.ProbeError, '其他用户') as error:
            self.inspect(root)
        self.assertEqual(error.exception.code, 'wrong_user')
        files['environ'].read_bytes.assert_not_called()

    def test_missing_process_is_distinct_from_permission_denied(self):
        root, proc, _ = target_process()
        proc.stat.side_effect = FileNotFoundError()
        with self.assertRaises(metadata.ProbeError) as error:
            self.inspect(root)
        self.assertEqual(error.exception.code, 'not_running')

    def test_protected_exe_and_environment_request_only_permission_fallback(self):
        for field in ['exe', 'environ']:
            with self.subTest(field=field):
                root, _, files = target_process()
                if field == 'exe':
                    files[field].resolve.side_effect = PermissionError()
                else:
                    files[field].read_bytes.side_effect = PermissionError()
                with self.assertRaises(metadata.ProbeError) as error:
                    self.inspect(root)
                self.assertEqual(error.exception.code, 'permission_denied')

    def test_pid_reuse_and_mid_read_replacement_are_rejected(self):
        for births in [['67890'], ['12345', '67890']]:
            with self.subTest(births=births):
                root, _, files = target_process()
                files['stat'].read_bytes.side_effect = [process_stat(value) for value in births]
                with self.assertRaises(metadata.ProbeError) as error:
                    self.inspect(root)
                self.assertEqual(error.exception.code, 'identity_changed')

    def test_changed_or_deleted_executable_is_not_guessed_by_basename(self):
        root, _, files = target_process()
        files['exe'].resolve.return_value = Path('/another/QQ/qq').resolve()
        with self.assertRaises(metadata.ProbeError) as error:
            self.inspect(root)
        self.assertEqual(error.exception.code, 'executable_changed')
        files['exe'].resolve.side_effect = FileNotFoundError()
        with self.assertRaisesRegex(metadata.ProbeError, '已删除'):
            self.inspect(root)

    def test_renderer_and_conflicting_account_are_rejected(self):
        for command, code in [(b'qq\0--type=renderer\0', 'not_main_process'),
                              (b'qq\0--qq=100010\0', 'account_mismatch')]:
            with self.subTest(command=command):
                root, _, files = target_process()
                files['cmdline'].read_bytes.return_value = command
                with self.assertRaises(metadata.ProbeError) as error:
                    self.inspect(root)
                self.assertEqual(error.exception.code, code)

    def test_existing_account_attribution_does_not_require_q_flag(self):
        root, _, files = target_process()
        files['cmdline'].read_bytes.return_value = b'qq\0--no-sandbox\0'
        self.assertEqual(self.inspect(root)['display'], ':2')

    def test_missing_forwarded_and_ambiguous_display_are_not_guessed(self):
        for environment, code in [(b'OTHER=value\0', 'missing_display'),
                                  (b'DISPLAY=localhost:10.0\0', 'missing_display'),
                                  (b'DISPLAY=:0\0DISPLAY=:2\0', 'ambiguous_session')]:
            with self.subTest(environment=environment):
                root, _, files = target_process()
                files['environ'].read_bytes.return_value = environment
                with self.assertRaises(metadata.ProbeError) as error:
                    self.inspect(root)
                self.assertEqual(error.exception.code, code)


if __name__ == '__main__':
    unittest.main()
