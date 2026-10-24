import importlib.util
from pathlib import Path
import stat
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('recovery', Path(__file__).with_name('novnc-recovery.py'))
recovery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recovery)


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


class TargetSessionTests(unittest.TestCase):
    def binding(self, display=':2'):
        return {'pid': 42, 'uid': 1000, 'start_time': '12345',
                'exe': str(Path('/opt/QQ/qq').resolve()), 'display': display, 'xauthority': None}

    def inspect(self, root, target=None):
        with mock.patch.object(recovery.os, 'getuid', return_value=1000, create=True):
            return recovery.target_display(Path('/opt/QQ/qq'), target or self.binding(), proc_root=root)

    def test_uses_verified_binding_without_reading_protected_metadata_as_user(self):
        root, _, files = target_process()
        files['exe'].resolve.side_effect = PermissionError()
        files['environ'].read_bytes.side_effect = PermissionError()
        self.assertEqual(self.inspect(root, self.binding(':106')), ':106')
        root.joinpath.assert_called_once_with('42')
        files['exe'].resolve.assert_not_called()
        files['environ'].read_bytes.assert_not_called()

    def test_binding_and_live_process_must_belong_to_original_user(self):
        root, proc, files = target_process()
        target = self.binding()
        target['uid'] = 2000
        with self.assertRaisesRegex(RuntimeError, '其他用户'):
            self.inspect(root, target)
        proc.stat.assert_not_called()
        proc.stat.return_value.st_uid = 2000
        with self.assertRaisesRegex(RuntimeError, '其他用户'):
            self.inspect(root)
        files['environ'].read_bytes.assert_not_called()

    def test_missing_process_is_reported_without_starting_services(self):
        root, proc, _ = target_process()
        proc.stat.side_effect = FileNotFoundError()
        with self.assertRaisesRegex(RuntimeError, '进程已退出'):
            self.inspect(root)

    def test_process_identity_permissions_are_reported(self):
        root, proc, _ = target_process()
        proc.stat.side_effect = PermissionError()
        with self.assertRaisesRegex(RuntimeError, '无权检查'):
            self.inspect(root)

    def test_pid_reuse_after_metadata_probe_is_rejected(self):
        root, _, files = target_process()
        files['stat'].read_bytes.return_value = process_stat('67890')
        with self.assertRaisesRegex(RuntimeError, '进程已变化'):
            self.inspect(root)

    def test_binding_executable_and_display_are_checked(self):
        root, _, _ = target_process()
        target = self.binding()
        target['exe'] = str(Path('/another/QQ/qq').resolve())
        with self.assertRaisesRegex(RuntimeError, '路径已变化'):
            self.inspect(root, target)
        with self.assertRaisesRegex(RuntimeError, '本地图形会话'):
            self.inspect(root, self.binding('localhost:10.0'))


class RecoveryTests(unittest.TestCase):
    def test_owned_vnc_service_can_be_identified_when_proc_exe_is_protected(self):
        _, proc, files = target_process()
        files['cmdline'].read_bytes.return_value = b'\0'.join([
            b'/usr/bin/x11vnc', b'-display', b':106', b'-rfbport', b'5999']) + b'\0'
        files['exe'].resolve.side_effect = PermissionError()
        files['comm'] = mock.Mock()
        files['comm'].read_bytes.return_value = b'x11vnc\n'
        root = mock.Mock()
        root.glob.return_value = [proc]
        with mock.patch.object(recovery, 'Path', side_effect=lambda value: root if value == '/proc' else Path(value)), \
             mock.patch.object(recovery.os, 'getuid', return_value=1000, create=True):
            items = recovery.processes()
            self.assertEqual(recovery.select_services(items, ':106'), (':106', True, 5999, None))
            proc.stat.return_value.st_uid = 2000
            self.assertEqual(recovery.processes(), [])

    def test_custom_display_and_ports_are_preserved(self):
        items = [(None, '/usr/bin/x11vnc', ['x11vnc', '-display', ':2', '-rfbport', '5999']),
                 (None, '/usr/bin/python3', ['/usr/bin/python3', '/usr/bin/websockify', '--web', '/novnc', '6099', 'localhost:5999'])]
        self.assertEqual(recovery.select_services(items, ':2'), (':2', True, 5999, (None, 6099, 5999)))

    def test_missing_vnc_uses_existing_proxy_target(self):
        items = [(None, '/usr/bin/python3', ['websockify', '6099', '127.0.0.1:5999'])]
        self.assertEqual(recovery.select_services(items, ':0'), (':0', False, 5999, (None, 6099, 5999)))

    def test_unrelated_command_text_is_not_a_proxy(self):
        items = [(None, '/usr/bin/python3', ['python3', '-c', "print('websockify')", '6081', 'localhost:5900'])]
        self.assertEqual(recovery.select_services(items, ':0'), (':0', False, 5900, None))

    def test_target_display_does_not_take_another_qq_session(self):
        items = [(None, '/opt/QQ/qq', ['qq', '-q', '10001']),
                 (None, '/opt/QQ/qq', ['qq', '-q', '10002']),
                 (None, '/usr/bin/x11vnc', ['x11vnc', '-display', ':0', '-rfbport', '5900']),
                 (None, '/usr/bin/x11vnc', ['x11vnc', '-display', ':2', '-rfbport', '5999']),
                 (None, '/usr/bin/python3', ['websockify', '6081', 'localhost:5900']),
                 (None, '/usr/bin/python3', ['websockify', '6099', 'localhost:5999'])]
        self.assertEqual(recovery.select_services(items, ':2'), (':2', True, 5999, (None, 6099, 5999)))

    def test_missing_target_vnc_does_not_reuse_another_display_proxy(self):
        items = [(None, '/usr/bin/x11vnc', ['x11vnc', '-display', ':0', '-rfbport', '5999']),
                 (None, '/usr/bin/python3', ['websockify', '6099', 'localhost:5999'])]
        self.assertEqual(recovery.select_services(items, ':2'), (':2', False, 5900, None))

    def test_ambiguous_target_vnc_services_are_not_guessed(self):
        items = [(None, '/usr/bin/x11vnc', ['x11vnc', '-display', ':2', '-rfbport', str(port)])
                 for port in [5900, 5999]]
        with self.assertRaises(RuntimeError):
            recovery.select_services(items, ':2')

    def test_recovery_launches_only_missing_vnc_services(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            root.joinpath('vnc.secret').write_text('test-only')
            root.joinpath('novnc').mkdir()
            root.joinpath('novnc/vnc.html').write_text('<html></html>')
            with mock.patch.object(recovery, 'target_display', return_value=':3'), \
                 mock.patch.object(recovery, 'processes', return_value=[]), \
                 mock.patch.object(recovery, 'select_services', return_value=(':3', False, 5900, None)), \
                 mock.patch.object(recovery, 'require_free_port'), \
                 mock.patch.object(recovery, 'ensure_vnc_secret') as ensure_secret, \
                 mock.patch.object(recovery, 'launch', return_value=SimpleNamespace(poll=lambda: None)) as launch, \
                 mock.patch.object(recovery, 'ready', return_value=True):
                target = {'xauthority': '/home/user/.Xauthority'}
                self.assertEqual(recovery.recover(root, root, root, root / 'vnc.secret', root / 'qq', target), 6081)
                commands = [call.args[0] for call in launch.call_args_list]
                self.assertEqual([command[0] for command in commands], ['x11vnc', 'websockify'])
                self.assertIn(':3', commands[0])
                self.assertIn('-localhost', commands[0])
                environment = launch.call_args_list[0].args[4]
                self.assertEqual(environment['DISPLAY'], ':3')
                self.assertEqual(environment['XAUTHORITY'], '/home/user/.Xauthority')
                ensure_secret.assert_called_once_with(root / 'vnc.secret')

    def test_healthy_services_are_reused_without_restart(self):
        with mock.patch.object(recovery, 'target_display', return_value=':0'), \
             mock.patch.object(recovery, 'processes', return_value=[]), \
             mock.patch.object(recovery, 'select_services', return_value=(':0', True, 5999, (None, 6099, 5999))), \
             mock.patch.object(recovery, 'launch') as launch, \
             mock.patch.object(recovery, 'ready', return_value=True):
            root = Path('.')
            self.assertEqual(recovery.recover(root, root, root, root / 'secret', root / 'qq', {}), 6099)
            launch.assert_not_called()

    def test_unresponsive_existing_services_report_failure_without_kill(self):
        with mock.patch.object(recovery, 'target_display', return_value=':0'), \
             mock.patch.object(recovery, 'processes', return_value=[]), \
             mock.patch.object(recovery, 'select_services', return_value=(':0', True, 5999, (None, 6099, 5999))), \
             mock.patch.object(recovery, 'launch') as launch, \
             mock.patch.object(recovery, 'ready', return_value=False), \
             mock.patch.object(recovery.time, 'monotonic', side_effect=[0, 0, 7]), \
             mock.patch.object(recovery.time, 'sleep'):
            root = Path('.')
            with self.assertRaises(RuntimeError): recovery.recover(root, root, root, root / 'secret', root / 'qq', {})
            launch.assert_not_called()


class SecretTests(unittest.TestCase):
    def test_missing_secret_is_created_exclusively_with_private_mode_and_random_value(self):
        with tempfile.TemporaryDirectory() as directory:
            secret = Path(directory) / 'vnc.secret'
            with mock.patch.object(recovery, 'validate_vnc_secret', side_effect=[FileNotFoundError(), None]), \
                 mock.patch.object(recovery.os, 'open', wraps=recovery.os.open) as create:
                recovery.ensure_vnc_secret(secret)
            self.assertGreaterEqual(len(secret.read_text().strip()), 32)
            flags, mode = create.call_args.args[1:]
            self.assertTrue(flags & recovery.os.O_EXCL)
            self.assertTrue(flags & recovery.os.O_CREAT)
            self.assertEqual(mode, 0o600)

    def test_existing_secret_is_never_rewritten(self):
        with tempfile.TemporaryDirectory() as directory:
            secret = Path(directory) / 'vnc.secret'
            secret.write_text('existing-test-value')
            with mock.patch.object(recovery, 'validate_vnc_secret'), \
                 mock.patch.object(recovery.os, 'open') as create:
                recovery.ensure_vnc_secret(secret)
            create.assert_not_called()
            self.assertEqual(secret.read_text(), 'existing-test-value')

    def test_creation_permission_error_is_reported_without_file(self):
        with tempfile.TemporaryDirectory() as directory:
            secret = Path(directory) / 'vnc.secret'
            with mock.patch.object(recovery, 'validate_vnc_secret', side_effect=FileNotFoundError()), \
                 mock.patch.object(recovery.os, 'open', side_effect=PermissionError()):
                with self.assertRaisesRegex(RuntimeError, '创建 VNC 密码文件失败'):
                    recovery.ensure_vnc_secret(secret)
            self.assertFalse(secret.exists())

    def test_failed_save_removes_only_its_new_file(self):
        with tempfile.TemporaryDirectory() as directory:
            secret = Path(directory) / 'vnc.secret'
            with mock.patch.object(recovery, 'validate_vnc_secret', side_effect=FileNotFoundError()), \
                 mock.patch.object(recovery.os, 'fsync', side_effect=OSError()):
                with self.assertRaisesRegex(RuntimeError, '保存 VNC 密码文件失败'):
                    recovery.ensure_vnc_secret(secret)
            self.assertFalse(secret.exists())

    def test_empty_foreign_and_permissive_files_are_rejected_without_overwrite(self):
        for uid, size, mode in [(1000, 0, 0o600), (2000, 32, 0o600), (1000, 32, 0o644)]:
            with self.subTest(uid=uid, size=size, mode=mode):
                secret = mock.Mock()
                secret.lstat.return_value = SimpleNamespace(st_uid=uid, st_size=size,
                                                            st_mode=stat.S_IFREG | mode)
                with mock.patch.object(recovery.os, 'getuid', return_value=1000, create=True):
                    with self.assertRaises(RuntimeError):
                        recovery.ensure_vnc_secret(secret)
                secret.open.assert_not_called()


if __name__ == '__main__':
    unittest.main()
