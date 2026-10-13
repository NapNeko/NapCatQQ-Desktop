import importlib.util
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('recovery', Path(__file__).with_name('novnc-recovery.py'))
recovery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recovery)


def qq_process(display=':0'):
    proc = mock.Mock()
    proc.joinpath.return_value.read_bytes.return_value = ('DISPLAY=' + display + '\0').encode()
    return proc, '/opt/QQ/qq', ['qq', '--no-sandbox']


class RecoveryTests(unittest.TestCase):
    def test_custom_display_and_ports_are_preserved(self):
        items = [qq_process(':2'), (None, '/usr/bin/x11vnc', ['x11vnc', '-display', ':2', '-rfbport', '5999']),
                 (None, '/usr/bin/python3', ['/usr/bin/python3', '/usr/bin/websockify', '--web', '/novnc', '6099', 'localhost:5999'])]
        self.assertEqual(recovery.select_services(items, '/opt/QQ/qq'), (':2', True, 5999, (None, 6099, 5999)))

    def test_missing_vnc_uses_existing_proxy_target(self):
        items = [qq_process(), (None, '/usr/bin/python3', ['websockify', '6099', '127.0.0.1:5999'])]
        self.assertEqual(recovery.select_services(items, '/opt/QQ/qq'), (':0', False, 5999, (None, 6099, 5999)))

    def test_unrelated_command_text_is_not_a_proxy(self):
        items = [qq_process(), (None, '/usr/bin/python3', ['python3', '-c', "print('websockify')", '6081', 'localhost:5900'])]
        self.assertEqual(recovery.select_services(items, '/opt/QQ/qq'), (':0', False, 5900, None))

    def test_ambiguous_qq_desktop_is_not_guessed(self):
        with self.assertRaises(RuntimeError):
            recovery.select_services([qq_process(':0'), qq_process(':2')], '/opt/QQ/qq')

    def test_recovery_launches_only_missing_vnc_services(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            root.joinpath('vnc.secret').write_text('test-only')
            root.joinpath('novnc').mkdir()
            root.joinpath('novnc/vnc.html').write_text('<html></html>')
            with mock.patch.object(recovery, 'processes', return_value=[]), \
                 mock.patch.object(recovery, 'select_services', return_value=(':3', False, 5900, None)), \
                 mock.patch.object(recovery, 'require_free_port'), \
                 mock.patch.object(recovery, 'launch', return_value=SimpleNamespace(poll=lambda: None)) as launch, \
                 mock.patch.object(recovery, 'ready', return_value=True):
                self.assertEqual(recovery.recover(root, root, root, root / 'vnc.secret', root / 'qq'), 6081)
                commands = [call.args[0] for call in launch.call_args_list]
                self.assertEqual([command[0] for command in commands], ['x11vnc', 'websockify'])
                self.assertIn(':3', commands[0])
                self.assertIn('-localhost', commands[0])

    def test_healthy_services_are_reused_without_restart(self):
        with mock.patch.object(recovery, 'processes', return_value=[]), \
             mock.patch.object(recovery, 'select_services', return_value=(':0', True, 5999, (None, 6099, 5999))), \
             mock.patch.object(recovery, 'launch') as launch, \
             mock.patch.object(recovery, 'ready', return_value=True):
            root = Path('.')
            self.assertEqual(recovery.recover(root, root, root, root / 'secret', root / 'qq'), 6099)
            launch.assert_not_called()

    def test_unresponsive_existing_services_report_failure_without_kill(self):
        with mock.patch.object(recovery, 'processes', return_value=[]), \
             mock.patch.object(recovery, 'select_services', return_value=(':0', True, 5999, (None, 6099, 5999))), \
             mock.patch.object(recovery, 'launch') as launch, \
             mock.patch.object(recovery, 'ready', return_value=False), \
             mock.patch.object(recovery.time, 'monotonic', side_effect=[0, 0, 7]), \
             mock.patch.object(recovery.time, 'sleep'):
            root = Path('.')
            with self.assertRaises(RuntimeError): recovery.recover(root, root, root, root / 'secret', root / 'qq')
            launch.assert_not_called()


if __name__ == '__main__':
    unittest.main()
